/// Simple immutable sendable map.
import option::extensions;

type imap<K: copy, V: copy> = ~[(K, V)];

// TODO: Replace this with something better. Frozen hashmap?
// Would be faster if we used a binary search, but that won't matter
// for our use cases.
impl imap_methods<K: copy, V: copy> for imap<K, V>
{
	fn size() -> uint
	{
		vec::len(self)
	}
	
	fn contains_key(key: K) -> bool
	{
		vec::find(self, |e| {tuple::first(e) == key}).is_some()
	}
	
	fn get(key: K) -> V
	{
		alt vec::find(self, |e| {tuple::first(e) == key})
		{
			option::some(e)
			{
				tuple::second(e)
			}
			option::none
			{
				fail(#fmt["Failed to find %?", key]);
			}
		}
	}
	
	fn find(key: K) -> option<V>
	{
		alt vec::find(self, |e| {tuple::first(e) == key})
		{
			option::some(e)
			{
				option::some(tuple::second(e))
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
			if !block(tuple::first(e), tuple::second(e))
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
			if !block(tuple::first(e))
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
			if !block(tuple::second(e))
			{
				break;
			}
		}
	}
}
