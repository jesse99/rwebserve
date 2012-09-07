//! Simple immutable and sendable multimap.
use option::extensions;

type imap<K: copy, V: copy> = ~[(K, V)];

trait immutable_map<K: copy, V: copy>
{
	fn size() -> uint;
	fn contains_key(key: K) -> bool;
	fn get(key: K) -> V;
	fn get_all(key: K) -> ~[V];
	fn find(key: K) -> option<V>;
	fn each(block: fn(K, V) -> bool);
	fn each_key(block: fn(K) -> bool);
	fn each_value(block: fn(V) -> bool);
}

// TODO: Replace this with something better. Frozen hashmap?
// But note that this is a multimap which hashmap doesn't currently support.
// Would be faster if we used a binary search, but that won't matter
// for our use cases.
impl<K: copy, V: copy> imap<K, V> : immutable_map<K, V>
{
	fn size() -> uint
	{
		vec::len(self)
	}
	
	fn contains_key(key: K) -> bool
	{
		vec::find(self, |e| {e.first() == key}).is_some()
	}
	
	/// Returns value for the first matching key or fails if no key was found.
	fn get(key: K) -> V
	{
		match vec::find(self, |e| {e.first() == key})
		{
			option::Some(e) =>
			{
				e.second()
			}
			option::None =>
			{
				fail(fmt!("Failed to find %?", key));
			}
		}
	}
	
	/// Returns all values matching key.
	fn get_all(key: K) -> ~[V]
	{
		do vec::filter_map(self)
		|e|
		{
			if e.first() == key
			{
				option::Some(e.second())
			}
			else
			{
				option::None
			}
		}
	}
	
	fn find(key: K) -> option<V>
	{
		match vec::find(self, |e| {e.first() == key})
		{
			option::Some(e) =>
			{
				option::Some(e.second())
			}
			option::None =>
			{
				option::None
			}
		}
	}
	
	fn each(block: fn(K, V) -> bool)
	{
		for vec::each(self)
		|e|
		{
			if !block(e.first(), e.second())
			{
				break;
			}
		}
	}
	
	fn each_key(block: fn(K) -> bool)
	{
		for vec::each(self)
		|e|
		{
			if !block(e.first())
			{
				break;
			}
		}
	}
	
	fn each_value(block: fn(V) -> bool)
	{
		for vec::each(self)
		|e|
		{
			if !block(e.second())
			{
				break;
			}
		}
	}
}
