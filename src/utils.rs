//! Misc functions used internally.

fn dump_string(title: ~str, text: ~str)
{
	io::println(#fmt["%s has %? bytes:", title, str::len(text)]);
	let mut i = 0u;
	while i < str::len(text)
	{
		// Print the byte offset for the start of the line.
		io::print(#fmt["%4X: ", i]);
		
		// Print the first 8 bytes as hex.
		let mut k = 0u;
		while k < 8u && i+k < str::len(text)
		{
			io::print(#fmt["%2X ", text[i+k] as uint]);
			k += 1u;
		}
		
		io::print(~"  ");
		
		// Print the second 8 bytes as hex.
		k = 0u;
		while k < 8u && i+8u+k < str::len(text)
		{
			io::print(#fmt["%2X ", text[i+8u+k] as uint]);
			k += 1u;
		}
		
		// Print the printable 16 characters as characters and
		// the unprintable characters as '.'.
		io::print(~"  ");
		k = 0u;
		while k < 16u && i < str::len(text)
		{
			if text[i] < ' ' as u8 || text[i] > '~' as u8
			{
				io::print(~".");
			}
			else
			{
				io::print(#fmt["%c", text[i] as char]);
			}
			k += 1u;
			i += 1u;
		}
		io::println(~"");
	}
}

fn truncate_str(s: ~str, max_chars: uint) -> ~str
{
	if s.len() > max_chars
	{
		s.substr(0, max_chars - 3) + "..."
	}
	else
	{
		s
	}
}

#[cfg(test)]
fn check_strs(actual: ~str, expected: ~str) -> bool
{
	if actual != expected
	{
		io::stderr().write_line(#fmt["Found '%s', but expected '%s'", actual, expected]);
		ret false;
	}
	ret true;
}

#[cfg(test)]
fn check_vectors<T>(actual: ~[T], expected: ~[T]) -> bool
{
	if actual != expected
	{
		io::stderr().write_line(#fmt["Found '%?', but expected '%?'", actual, expected]);
		ret false;
	}
	ret true;
}
