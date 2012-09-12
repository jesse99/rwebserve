//! Simple immutable and sendable multimap.
use core::cmp;

type IMap<K: Copy, V: Copy> = ~[(K, V)];

trait ImmutableMap<K: Copy, V: Copy>
{
	pure fn size() -> uint;
	pure fn contains_key(key: K) -> bool;
	pure fn get(key: K) -> V;
	pure fn get_all(key: K) -> ~[V];
	pure fn find(key: K) -> Option<V>;
	pure fn each(block: fn(K, V) -> bool);
	pure fn each_key(block: fn(K) -> bool);
	pure fn each_value(block: fn(V) -> bool);
}

// TODO: Replace this with something better. Frozen hashmap?
// But note that this is a multimap which hashmap doesn't currently support.
// Would be faster if we used a binary search, but that won't matter
// for our use cases.
impl<K: Copy core::cmp::Eq, V: Copy> IMap<K, V> : ImmutableMap<K, V>
{
	pure fn size() -> uint
	{
		vec::len(self)
	}
	
	pure fn contains_key(key: K) -> bool
	{
		vec::find(self, |e| {e.first() == key}).is_some()
	}
	
	/// Returns value for the first matching key or fails if no key was found.
	pure fn get(key: K) -> V
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
	pure fn get_all(key: K) -> ~[V]
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
	
	pure fn find(key: K) -> Option<V>
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
	
	pure fn each(block: fn(K, V) -> bool)
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
	
	pure fn each_key(block: fn(K) -> bool)
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
	
	pure fn each_value(block: fn(V) -> bool)
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
