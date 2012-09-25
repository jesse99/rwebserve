//! Misc functions used internally.
use io::WriterUtil;
use std::map::*;
use path::Path;

// The url should be the path component of an URL. It will usually be
// an absolute path which is actually relative to root.
fn url_to_path(root: &Path, url: &str) -> Path
{
	let path = path::from_str(
		if url.is_not_empty() && url.char_at(0) == '/'
		{
			url.slice(1, url.len())
		}
		else
		{
			url.to_unique()
		}
	);
	root.push_rel(&path)
}

fn boxed_hash_from_strs<V: Copy>(items: &[(~str, V)]) -> HashMap<@~str, V>
{
	let table = HashMap();
	for items.each
	|item|
	{
		table.insert(@item.first(), item.second());
	}
	table
}

fn to_boxed_str_hash(items: &[(~str, ~str)]) -> HashMap<@~str, @~str>
{
	let table = HashMap();
	for items.each
	|item|
	{
		table.insert(@item.first(), @item.second());
	}
	table
}

fn dump_string(title: ~str, text: ~str)
{
	io::println(fmt!("%s has %? bytes:", title, str::len(text)));
	let mut i = 0u;
	while i < str::len(text)
	{
		// Print the byte offset for the start of the line.
		io::print(fmt!("%4X: ", i));
		
		// Print the first 8 bytes as hex.
		let mut k = 0u;
		while k < 8u && i+k < str::len(text)
		{
			io::print(fmt!("%2X ", text[i+k] as uint));
			k += 1u;
		}
		
		io::print(~"  ");
		
		// Print the second 8 bytes as hex.
		k = 0u;
		while k < 8u && i+8u+k < str::len(text)
		{
			io::print(fmt!("%2X ", text[i+8u+k] as uint));
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
				io::print(fmt!("%c", text[i] as char));
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
		copy s
	}
}

#[cfg(test)]
fn check_strs(actual: ~str, expected: ~str) -> bool
{
	if actual != expected
	{
		io::stderr().write_line(fmt!("Found '%s', but expected '%s'", actual, expected));
		return false;
	}
	return true;
}

#[cfg(test)]
fn check_vectors<T: cmp::Eq>(actual: ~[T], expected: ~[T]) -> bool
{
	if actual != expected
	{
		io::stderr().write_line(fmt!("Found '%?', but expected '%?'", actual, expected));
		return false;
	}
	return true;
}
