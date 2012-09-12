use std::map::*;

export Component, compile, match_template;

// Components of a template path.
enum Component
{
	Literal(~str),		// match iff the component is str
	Variable(~str),		// matches an arbitrary component, str will be the key name
	Trailer(~str)		// matches zero or more components, str will be the key name
}

// TODO: This is hopefully temporary: at some point rust should again be able to compare enums without assistence.
impl Component : cmp::Eq
{
	pure fn eq(&&rhs: Component) -> bool
	{
		fmt!("%?", self) == fmt!("%?", rhs)
	}
	
	pure fn ne(&&rhs: Component) -> bool
	{
		fmt!("%?", self) != fmt!("%?", rhs)
	}
}

// Template should correspond to the path component of an URI.
// Note that the template need not have variable components.
// Templates look like:
//    /blueprint/{site}/{building}		site and building match any (single) component
//    /csv/*path						path matches zero or more components
fn compile(template: ~str) -> ~[Component]
{
	let parts = str::split_char_nonempty(template, '/');
	
	let mut result = do vec::map(parts)
	|part|
	{
		if str::starts_with(part, "{") && str::ends_with(part, "}")
		{
			Variable(str::slice(part, 1u, str::len(part)-1u))
		}
		else
		{
			Literal(part)
		}
	};
	
	if vec::is_not_empty(parts)
	{
		let last = 	vec::last(parts);
		if str::starts_with(last, "*")
		{
			vec::pop(result);
			vec::push(result, Trailer(str::slice(last, 1u, str::len(last))));
		}
	}
	
	return result;
}

// Path should be the path component of an URI.
// Components should be the result of a call to compile.
// Result will be non-empty iff all of the components in path match the specified components.
// On matches result will have keys matching any variable names as well as a "fullpath" key matching the entire path.
fn match_template(path: ~str, components: ~[Component]) -> hashmap<@~str, @~str>
{
	let parts = str::split_char_nonempty(path, '/');
	
	let mut i = 0u;
	let result = std::map::box_str_hash();
	while i < vec::len(components)
	{
		if i == vec::len(parts)
		{
			return std::map::box_str_hash();			// ran out of parts to match
		}
		
		match components[i]
		{
			Literal(s) =>
			{
				if parts[i] != s
				{
					return std::map::box_str_hash();	// match failed
				}
			}
			Variable(s) =>
			{
				result.insert(@s, @parts[i]);
			}
			Trailer(s) =>
			{
				let path = vec::slice(parts, i, vec::len(parts));
				result.insert(@s, @str::connect(path, ~"/"));
				i = vec::len(parts) - 1u;
			}
		}
		i += 1u;
	}
	
	if i != vec::len(parts)
	{
		return std::map::box_str_hash();				// not all parts were matched
	}
	
	result.insert(@~"fullpath", @path);
	return result;
}

// ---- Unit Tests ------------------------------------------------------------
#[test]
fn compile_literal()
{
	let template = ~"/foo/bar/baz";
	let components = compile(template);
	//io::println(fmt!("%?", components));
	
	assert components[0] == Literal(~"foo");
	assert components[1] == Literal(~"bar");
	assert components[2] == Literal(~"baz");
	assert vec::len(components) == 3u;
}

#[test]
fn compile_variable()
{
	let template = ~"/foo/{ba}r/ba{z}";
	let components = compile(template);
	//io::println(fmt!("%?", components));
	
	assert components[0] == Literal(~"foo");
	assert components[1] == Literal(~"{ba}r");
	assert components[2] == Literal(~"ba{z}");
	assert vec::len(components) == 3u;
}

#[test]
fn compile_non_variable()
{
	let template = ~"/foo/{bar}/{baz}";
	let components = compile(template);
	//io::println(fmt!("%?", components));
	
	assert components[0] == Literal(~"foo");
	assert components[1] == Variable(~"bar");
	assert components[2] == Variable(~"baz");
	assert vec::len(components) == 3u;
}

#[test]
fn compile_path()
{
	let template = ~"/foo/*path";
	let components = compile(template);
	//io::println(fmt!("%?", components));
	
	assert components[0] == Literal(~"foo");
	assert components[1] == Trailer(~"path");
	assert vec::len(components) == 2u;
}

#[test]
fn compile_non_path()
{
	let template = ~"/foo/*lame/url";
	let components = compile(template);
	//io::println(fmt!("%?", components));
	
	assert components[0] == Literal(~"foo");
	assert components[1] == Literal(~"*lame");
	assert components[2] == Literal(~"url");
	assert vec::len(components) == 3u;
}

#[test]
fn match_root()
{
	let path = ~"/";
	let template = ~"/";
	let components = compile(template);
	let m = match_template(path, components);
	assert m.get(@~"fullpath") == @~"/";
	assert m.size() == 1u;
	
	let path = ~"/foo";
	let m = match_template(path, components);
	assert m.size() == 0u;
}

#[test]
fn match_literals()
{
	let path = ~"/foo/bar/baz";
	let template = ~"/foo/bar/baz";
	let components = compile(template);
	let m = match_template(path, components);
	
	assert m.get(@~"fullpath") == @~"/foo/bar/baz";
	assert m.size() == 1u;
}

#[test]
fn match_non_literals()
{
	let path = ~"/foo/bar/baz";
	let template = ~"/foo/bar/baz/flob";
	let components = compile(template);
	let m = match_template(path, components);
	assert m.size() == 0u;
	
	let path = ~"/foo/bar/baz";
	let template = ~"/foo";
	let components = compile(template);
	let m = match_template(path, components);
	assert m.size() == 0u;
}

#[test]
fn match_variables()
{
	let path = ~"/foo/alpha/beta";
	let template = ~"/foo/{bar}/{baz}";
	let components = compile(template);
	let m = match_template(path, components);
	
	assert m.get(@~"fullpath") == @~"/foo/alpha/beta";
	assert m.get(@~"bar") == @~"alpha";
	assert m.get(@~"baz") == @~"beta";
	assert m.size() == 3u;
}

#[test]
fn match_paths()
{
	let path = ~"/foo/alpha/beta";
	let template = ~"/foo/*path";
	let components = compile(template);
	let m = match_template(path, components);
	
	assert m.get(@~"fullpath") == @~"/foo/alpha/beta";
	assert m.get(@~"path") == @~"alpha/beta";
	assert m.size() == 2u;
}

#[test]
fn match_empty_path()
{
	// Empty path isn't useful so we don't match it.
	let path = ~"/foo/";
	let template = ~"/foo/*path";
	let components = compile(template);
	let m = match_template(path, components);
	
	assert m.size() == 0u;
}
