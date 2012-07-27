//! Simple immutable and sendable multimap.
import option::extensions;

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
impl imap_methods<K: copy, V: copy> of immutable_map<K, V> for imap<K, V>
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
		alt vec::find(self, |e| {e.first() == key})
		{
			option::some(e)
			{
				e.second()
			}
			option::none
			{
				fail(#fmt["Failed to find %?", key]);
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
				option::some(e.second())
			}
			else
			{
				option::none
			}
		}
	}
	
	fn find(key: K) -> option<V>
	{
		alt vec::find(self, |e| {e.first() == key})
		{
			option::some(e)
			{
				option::some(e.second())
			}
			option::none
			{
				option::none
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
