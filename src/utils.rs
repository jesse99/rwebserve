//! Misc functions used internally.
use core::path::{GenericPath};
use core::send_map::linear::{LinearMap, linear_map_with_capacity};
use io::WriterUtil;

// The url should be the path component of an URL. It will usually be
// an absolute path which is actually relative to root.
pub fn url_to_path(root: &Path, url: &str) -> Path
{
	let path = GenericPath::from_str(
		if url.is_not_empty() && url.char_at(0) == '/'
		{
			url.slice(1, url.len())
		}
		else
		{
			url.to_owned()
		}
	);
	root.push_rel(&path)
}

pub fn linear_map_from_vector<K: cmp::Eq hash::Hash to_bytes::IterBytes, V: Copy>
	(vector: &[(K, V)]) -> LinearMap<K, V>
{
	let mut map = linear_map_with_capacity(vector.len());
	
	for vector.each |&(key, value)|
	{
		map.insert(key, value);
	}
	
	map
}

pub fn vector_from_linear_map<K: cmp::Eq hash::Hash to_bytes::IterBytes Copy, V: Copy>
	(map: &LinearMap<K, V>) -> ~[(K, V)]
{
	let mut vector = ~[];
	vec::reserve(&mut vector, map.len());
	
	for map.each |key, value|
	{
		vector.push((copy *key, copy *value));
	}
	
	vector
}

pub fn dump_string(title: ~str, text: ~str)
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

pub fn truncate_str(s: &str, max_chars: uint) -> ~str
{
	if s.len() > max_chars
	{
		s.substr(0, max_chars - 3) + "..."
	}
	else
	{
		s.to_owned()
	}
}

#[cfg(test)]
pub fn check_strs(actual: &str, expected: &str) -> bool
{
	if actual != expected
	{
		io::stderr().write_line(fmt!("Found '%s', but expected '%s'", actual, expected));
		return false;
	}
	return true;
}

#[cfg(test)]
pub fn check_vectors<T: cmp::Eq>(actual: &[T], expected: &[T]) -> bool
{
	if actual != expected
	{
		io::stderr().write_line(fmt!("Found '%?', but expected '%?'", actual, expected));
		return false;
	}
	return true;
}
