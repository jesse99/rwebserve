// Copy of (most of) rparse::parsers.rs.
// TODO: remove this one https://github.com/mozilla/rust/issues/4260 is fixed.
use rparse::{ParseStatus, ParseFailed, CharParsers, StringParsers, GenericParsers, Combinators};
use rparse::{Parser, State, Status, Succeeded, Failed};
use rparse::{EOT, is_alpha, is_digit, is_alphanum, is_print, is_whitespace};
use core::str::CharRange;

// ---- weird parsers -----------------------------------------------------------------------------
// Returns a parser which matches the end of the input.
// Clients should use everything instead of this.
#[doc(hidden)]
pub fn eot() -> Parser<()>
{
	|input: State|
	{
		if input.text[input.index] == EOT
		{
			result::Ok(Succeeded {new_state: State {index: input.index + 1u, ..input}, value: ()})
		}
		else
		{
			result::Err(Failed {old_state: input, err_state: input, mesg: @~"EOT"})
		}
	}
}

// ---- char parsers ------------------------------------------------------------------------------
/// Consumes a character which must satisfy the predicate.
/// Returns the matched character.
pub fn anycp(predicate: fn@ (char) -> bool) -> Parser<char>
{
	|input: State| {
		let mut i = input.index;
		if input.text[i] != EOT && predicate(input.text[i])
		{
			i += 1u;
		}
		
		if i > input.index
		{
			result::Ok(Succeeded {new_state: State {index: i, ..input}, value: input.text[input.index]})
		}
		else
		{
			result::Err(Failed {old_state: input, err_state: State {index: i, ..input}, mesg: @~""})
		}
	}
}

// ---- string parsers ----------------------------------------------------------------------------
// It would be a lot more elegant if match0, match1, and co were removed
// and users relied on composition to build the sort of parsers that they
// want. However in practice this is not such a good idea:
// 1) Matching is a very common operation, but instead of something simple
// like:
//    match0(p)
// users would have to write something like:
//    match(p).r0().str()
// 2) Generating an array of characters and then converting them into a string
// is much slower than updating a mutable string.
// 3) Debugging a parser is simpler if users can use higher level building
// blocks (TODO: though maybe we can somehow ignore or collapse low
// level parsers when logging).

/// Consumes zero or more characters matching the predicate.
/// Returns the matched characters. 
/// 
/// Note that this does not increment line.
pub fn match0(predicate: fn@ (char) -> bool) -> Parser<@~str>
{
	|input: State|
	{
		let mut i = input.index;
		while input.text[i] != EOT && predicate(input.text[i])
		{
			i += 1u;
		}
		
		let text = str::from_chars(vec::slice(input.text, input.index, i));
		result::Ok(Succeeded {new_state: State {index: i, ..input}, value: @text})
	}
}

/// Consumes one or more characters matching the predicate.
/// Returns the matched characters. 
/// 
/// Note that this does not increment line.
pub fn match1(predicate: fn@ (char) -> bool) -> Parser<@~str>
{
	|input: State|
	{
		let mut i = input.index;
		while input.text[i] != EOT && predicate(input.text[i])
		{
			i += 1u;
		}
		
		if i > input.index
		{
			let text = str::from_chars(vec::slice(input.text, input.index, i));
			result::Ok(Succeeded {new_state: State {index: i, ..input}, value: @text})
		}
		else
		{
			result::Err(Failed {old_state: input, err_state: State {index: i, ..input}, mesg: @~""})
		}
	}
}

/// match1_0 := prefix+ suffix*
pub fn match1_0(prefix: fn@ (char) -> bool, suffix: fn@ (char) -> bool) -> Parser<@~str>
{
	let prefix = match1(prefix);
	let suffix = match0(suffix);
	prefix.thene(|p| suffix.thene(|s| ret(@(*p + *s))))
}

/// optional_str := e?
///
/// Returns an empty string on failure.
pub fn optional_str(parser: Parser<@~str>) -> Parser<@~str>
{
	|input: State|
	{
		match parser(input)
		{
			result::Ok(ref pass)		=> result::Ok(Succeeded {new_state: pass.new_state, value: pass.value}),
			result::Err(ref _failure)		=> result::Ok(Succeeded {new_state: input, value: @~""}),
		}
	}
}

/// Calls fun once and matches the number of characters returned by fun. 
/// 
/// This does increment line.  Note that this succeeds even if zero characters are matched.
///
/// # Fun's are typically written like this:
///
/// ~~~
/// fn to_new_line(chars: @[char], index: uint) -> uint
/// {
///     let mut i = index;
///     loop
///     {
///         // Chars will always have an EOT character. If we hit it then
///         // we failed to find a new-line character so match nothing. 
///         if chars[i] == EOT
///         {
///             return 0;
///         }
///         else if chars[i] == '\r' || chars[i] == '\n'
///         {
///             // Match all the characters up to, but not including, the first new line.
///             return i - index;
///         }
///         else
///         {
///             i += 1;
///         }
///     }
/// }
/// ~~~
pub fn scan(fun: fn@ (@[char], uint) -> uint) -> Parser<@~str>
{
	|input: State|
	{
		let mut i = input.index;
		let mut line = input.line;
		
		let count = fun(input.text, i);
		if count > 0u && input.text[i] != EOT		// EOT check makes it easier to write funs that do stuff like matching chars that are not something
		{
			for uint::range(0u, count)
			|_k| {
				if input.text[i] == '\r'
				{
					line += 1;
				}
				else if input.text[i] == '\n' && (i == 0u || input.text[i-1u] != '\r')
				{
					line += 1;
				}
				i += 1u;
			}
			let text = str::from_chars(vec::slice(input.text, input.index, i));
			result::Ok(Succeeded {new_state: State {index: i, line: line, ..input}, value: @text})
		}
		else
		{
			result::Ok(Succeeded {new_state: State {index: i, line: line, ..input}, value: @~""})
		}
	}
}


/// If all the parsers are successful then the matched text is returned.
pub fn seq2_ret_str<T0: Copy Durable, T1: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>) -> Parser<@~str>
{
	|input: State|
	{
		match p0.then(p1)(input)
		{
			result::Ok(ref pass) =>
			{
				let text = str::from_chars(vec::slice(input.text, input.index, pass.new_state.index));
				result::Ok(Succeeded {new_state: pass.new_state, value: @text})
			}
			result::Err(ref failure) =>
			{
				result::Err(Failed {old_state: input, ..*failure})
			}
		}
	}
}

/// If all the parsers are successful then the matched text is returned.
pub fn seq3_ret_str<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>) -> Parser<@~str>
{
	|input: State|
	{
		match p0.then(p1). then(p2)(input)
		{
			result::Ok(ref pass) =>
			{
				let text = str::from_chars(vec::slice(input.text, input.index, pass.new_state.index));
				result::Ok(Succeeded {new_state: pass.new_state, value: @text})
			}
			result::Err(ref failure) =>
			{
				result::Err(Failed {old_state: input, ..*failure})
			}
		}
	}
}

/// If all the parsers are successful then the matched text is returned.
pub fn seq4_ret_str<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>, p3: Parser<T3>) -> Parser<@~str>
{
	|input: State| {
		match p0.then(p1). then(p2).then(p3)(input)
		{
			result::Ok(ref pass) =>
			{
				let text = str::from_chars(vec::slice(input.text, input.index, pass.new_state.index));
				result::Ok(Succeeded {new_state: pass.new_state, value: @text})
			}
			result::Err(ref failure) =>
			{
				result::Err(Failed {old_state: input, ..*failure})
			}
		}
	}
}

/// If all the parsers are successful then the matched text is returned.
pub fn seq5_ret_str<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, T4: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>, p3: Parser<T3>, p4: Parser<T4>) -> Parser<@~str>
{
	|input: State| {
		match p0.then(p1). then(p2).then(p3).then(p4)(input)
		{
			result::Ok(ref pass) =>
			{
				let text = str::from_chars(vec::slice(input.text, input.index, pass.new_state.index));
				result::Ok(Succeeded {new_state: pass.new_state, value: @text})
			}
			result::Err(ref failure) =>
			{
				result::Err(Failed {old_state: input, ..*failure})
			}
		}
	}
}

// ---- generic parsers ---------------------------------------------------------------------------
/// Returns a parser which always fails.
pub fn fails<T: Copy Durable>(mesg: &str) -> Parser<T>
{
	let mesg = mesg.to_owned();
	|input: State| result::Err(Failed {old_state: input, err_state: input, mesg: @copy mesg})
}

/// Parses with the aid of a pointer to a parser (useful for things like parenthesized expressions).
///
/// # Usage is like this:
///
/// ~~~
/// // create a pointer that we can initialize later with the real expr parser
/// let expr_ptr = @mut ret(0i);
/// let expr_ref = forward_ref(expr_ptr);
/// 
/// // expr_ref can be used to parse expressions
/// 
/// // initialize the expr_ptr with the real parser
/// *expr_ptr = expr;
/// ~~~
pub fn forward_ref<T: Copy Durable>(parser: @mut Parser<T>) -> Parser<T>
{
	|input: State| (*parser)(input)
}

pub pure fn at_connect(v: &[@~str], sep: &str) -> ~str
{
	let mut s = ~"", first = true;
	for vec::each(v) |ss|
	{
		if first {first = false;} else {unsafe {str::push_str(&mut s, sep);}}
		unsafe {str::push_str(&mut s, **ss)};
	}
	return s;
}

/// or_v := e0 | e1 | …
/// 
/// This is a version of or that is nicer to use when there are more than two alternatives.
pub fn or_v<T: Copy Durable>(parsers: @~[Parser<T>]) -> Parser<T>
{
	// A recursive algorithm would be a lot simpler, but it's not clear how that could
	// produce good error messages.
	assert vec::is_not_empty(*parsers);
	
	|input: State|
	{
		let mut result: Option<Status<T>> = None;
		let mut errors = ~[];
		let mut max_index = uint::max_value;
		let mut i = 0u;
		while i < vec::len(*parsers) && option::is_none(&result)
		{
			match parsers[i](input)
			{
				result::Ok(ref pass) =>
				{
					result = option::Some(result::Ok(*pass));
				}
				result::Err(ref failure) =>
				{
					if failure.err_state.index > max_index || max_index == uint::max_value
					{
						errors = ~[failure.mesg];
						max_index = failure.err_state.index;
					}
					else if failure.err_state.index == max_index
					{
						vec::push(&mut errors, failure.mesg);
					}
				}
			}
			i += 1u;
		}
		
		if option::is_some(&result)
		{
			option::get(result)
		}
		else
		{
			let errs = do vec::filter(errors) |s| {str::is_not_empty(**s)};
			let mesg = at_connect(errs, ~" or ");
			result::Err(Failed {old_state: input, err_state: State {index: max_index, ..input}, mesg: @mesg})
		}
	}
}

/// Returns a parser which always succeeds, but does not consume any input.
#[allow(deprecated_mode)]		// TODO: probably need to use &T instead
pub fn ret<T: Copy Durable>(value: T) -> Parser<T>
{
	|input: State| result::Ok(Succeeded {new_state: input, value: value})
}

/// seq2 := e0 e1
pub fn seq2<T0: Copy Durable, T1: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, eval: fn@ (T0, T1) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
		match eval(a0, a1)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}
}

/// seq3 := e0 e1 e2
pub fn seq3<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, eval: fn@ (T0, T1, T2) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
		match eval(a0, a1, a2)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}
}

/// seq4 := e0 e1 e2 e3
pub fn seq4<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, parser3: Parser<T3>, eval: fn@ (T0, T1, T2, T3) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
	do parser3.thene() |a3| {
		match eval(a0, a1, a2, a3)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}}
}

/// seq5 := e0 e1 e2 e3 e4
pub fn seq5<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, T4: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, parser3: Parser<T3>, parser4: Parser<T4>, eval: fn@ (T0, T1, T2, T3, T4) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
	do parser3.thene() |a3| {
	do parser4.thene() |a4| {
		match eval(a0, a1, a2, a3, a4)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}}}
}

/// seq6 := e0 e1 e2 e3 e4 e5
pub fn seq6<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, T4: Copy Durable, T5: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, parser3: Parser<T3>, parser4: Parser<T4>, parser5: Parser<T5>, eval: fn@ (T0, T1, T2, T3, T4, T5) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
	do parser3.thene() |a3| {
	do parser4.thene() |a4| {
	do parser5.thene() |a5| {
		match eval(a0, a1, a2, a3, a4, a5)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}}}}
}

/// seq7 := e0 e1 e2 e3 e4 e5 e6
pub fn seq7<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, T4: Copy Durable, T5: Copy Durable, T6: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, parser3: Parser<T3>, parser4: Parser<T4>, parser5: Parser<T5>, parser6: Parser<T6>, eval: fn@ (T0, T1, T2, T3, T4, T5, T6) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
	do parser3.thene() |a3| {
	do parser4.thene() |a4| {
	do parser5.thene() |a5| {
	do parser6.thene() |a6| {
		match eval(a0, a1, a2, a3, a4, a5, a6)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}}}}}
}

/// seq8 := e0 e1 e2 e3 e4 e5 e6 e7
pub fn seq8<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, T4: Copy Durable, T5: Copy Durable, T6: Copy Durable, T7: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, parser3: Parser<T3>, parser4: Parser<T4>, parser5: Parser<T5>, parser6: Parser<T6>, parser7: Parser<T7>, eval: fn@ (T0, T1, T2, T3, T4, T5, T6, T7) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
	do parser3.thene() |a3| {
	do parser4.thene() |a4| {
	do parser5.thene() |a5| {
	do parser6.thene() |a6| {
	do parser7.thene() |a7| {
		match eval(a0, a1, a2, a3, a4, a5, a6, a7)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}}}}}}
}

/// seq9 := e0 e1 e2 e3 e4 e5 e6 e7 e8
pub fn seq9<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable, T4: Copy Durable, T5: Copy Durable, T6: Copy Durable, T7: Copy Durable, T8: Copy Durable, R: Copy Durable>
	(parser0: Parser<T0>, parser1: Parser<T1>, parser2: Parser<T2>, parser3: Parser<T3>, parser4: Parser<T4>, parser5: Parser<T5>, parser6: Parser<T6>, parser7: Parser<T7>, parser8: Parser<T8>, eval: fn@ (T0, T1, T2, T3, T4, T5, T6, T7, T8) -> result::Result<R, @~str>) -> Parser<R>
{
	do parser0.thene() |a0| {
	do parser1.thene() |a1| {
	do parser2.thene() |a2| {
	do parser3.thene() |a3| {
	do parser4.thene() |a4| {
	do parser5.thene() |a5| {
	do parser6.thene() |a6| {
	do parser7.thene() |a7| {
	do parser8.thene() |a8| {
		match eval(a0, a1, a2, a3, a4, a5, a6, a7, a8)
		{
			result::Ok(ref value) =>
			{
				ret(*value)
			}
			result::Err(mesg) =>
			{
				fails(*mesg)
			}
		}
	}}}}}}}}}
}

/// seq2_ret0 := e0 e1
pub fn seq2_ret0<T0: Copy Durable, T1: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>) -> Parser<T0>
{
	seq2(p0, p1, |a0, _a1| result::Ok(a0))
}

/// seq2_ret1 := e0 e1
pub fn seq2_ret1<T0: Copy Durable, T1: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>) -> Parser<T1>
{
	seq2(p0, p1, |_a0, a1| result::Ok(a1))
}

/// seq3_ret0 := e0 e1 e2
pub fn seq3_ret0<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>) -> Parser<T0>
{
	seq3(p0, p1, p2, |a0, _a1, _a2| result::Ok(a0))
}

/// seq3_ret1 := e0 e1 e2
pub fn seq3_ret1<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>) -> Parser<T1>
{
	seq3(p0, p1, p2, |_a0, a1, _a2| result::Ok(a1))
}

/// seq3_ret2 := e0 e1 e2
pub fn seq3_ret2<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>) -> Parser<T2>
{
	seq3(p0, p1, p2, |_a0, _a1, a2| result::Ok(a2))
}

/// seq4_ret0 := e0 e1 e2 e3
pub fn seq4_ret0<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>, p3: Parser<T3>) -> Parser<T0>
{
	seq4(p0, p1, p2, p3, |a0, _a1, _a2, _a3| result::Ok(a0))
}

/// seq4_ret1 := e0 e1 e2 e3
pub fn seq4_ret1<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>, p3: Parser<T3>) -> Parser<T1>
{
	seq4(p0, p1, p2, p3, |_a0, a1, _a2, _a3| result::Ok(a1))
}

/// seq4_ret2 := e0 e1 e2 e3
pub fn seq4_ret2<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>, p3: Parser<T3>) -> Parser<T2>
{
	seq4(p0, p1, p2, p3, |_a0, _a1, a2, _a3| result::Ok(a2))
}

/// seq4_ret3 := e0 e1 e2 e3
pub fn seq4_ret3<T0: Copy Durable, T1: Copy Durable, T2: Copy Durable, T3: Copy Durable>(p0: Parser<T0>, p1: Parser<T1>, p2: Parser<T2>, p3: Parser<T3>) -> Parser<T3>
{
	seq4(p0, p1, p2, p3, |_a0, _a1, _a2, a3| result::Ok(a3))
}

// chain_suffix := (op e)*
#[doc(hidden)]
pub fn chain_suffix<T: Copy Durable, U: Copy Durable>(parser: Parser<T>, op: Parser<U>) -> Parser<@~[(U, T)]>
{
	let q = op.thene(
	|operator|
	{
		parser.thene(
		|value|
		{
			ret((operator, value))
		})
	});
	q.r0()
}

// When using tag it can be useful to use empty messages for interior parsers
// so we need to handle that case.
#[doc(hidden)]
pub fn or_mesg(mesg1: @~str, mesg2: @~str) -> @~str
{
	if str::is_not_empty(*mesg1) && str::is_not_empty(*mesg2)
	{
		@(*mesg1 + " or " + *mesg2)
	}
	else if str::is_not_empty(*mesg1)
	{
		mesg1
	}
	else if str::is_not_empty(*mesg2)
	{
		mesg2
	}
	else
	{
		@~""
	}
}

pub trait Combinators2<T: Copy Durable>
{
	fn everything2<U: Copy Durable>(space: Parser<U>) -> Parser<T>;
}

pub impl<T: Copy Durable> Parser<T> : Combinators2<T>
{
	fn everything2<U: Copy Durable>(space: Parser<U>) -> Parser<T>
	{
		seq3_ret1(space, self, eot())
	}
}
