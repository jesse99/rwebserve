use io::{WriterUtil};
use rparse::rparse::*;
use std::map;
use imap::*;

export HttpRequest, make_parser;

// This needs to be a sendable type.
struct HttpRequest
{
	let method: ~str;				// per 5.1.1 these are case sensitive
	let major_version: int;
	let minor_version: int;
	let url: ~str;
	let headers: ~[(~str, ~str)];		// these are not case sensitive so we lower case them
	let body: ~str;					// set elsewhere
}

fn is_hex(octet: u8) -> bool
{
	let ch = octet as char;
	return (ch >= 'a' && ch <= 'f') || (ch >= 'A' && ch <= 'F') || (ch >= '0' && ch <= '9');
}

fn to_int(octet: u8) -> uint
{
	let ch = octet as char;
	if ch >= 'a' && ch <= 'f'
	{
		return (ch - 'a') as uint + 10u;
	}
	else if ch >= 'A' && ch <= 'F'
	{
		return (ch - 'A') as uint + 10u;
	}
	else
	{
		return (ch - '0') as uint;
	}
}

fn decode(url: ~str) -> ~str
{
	let mut result = ~"";
	let mut i = 0u;
	str::reserve(result, str::len(url));
	
	while i < str::len(url)
	{
		if i+1u < str::len(url) && url[i] == '%' as u8 && is_hex(url[i+1u])
		{
			i += 1u;
			let mut code_point = 0u;
			
			if i < str::len(url) && is_hex(url[i])
			{
				code_point = (code_point << 4) | to_int(url[i]);
				i += 1u;
			}
			if i < str::len(url) && is_hex(url[i])
			{
				code_point = (code_point << 4) | to_int(url[i]);
				i += 1u;
			}
			
			str::push_char(result, code_point as char);
		}
		else
		{
			str::push_char(result, url[i] as char);
			i += 1u;
		}
	}
	
	return result;
}

// TODO: 
// Server, User-Agent, and Via values can have comments
// double quotes can be used with header values that use separators
fn request_parser() -> Parser<HttpRequest>
{
	let ws = " \t".anyc();
	let lws = ws.r0();
	let crnl = "\r\n".lit();
	
	// url := [^ ]+
	let url = match1(|c| {c != ' '});
	
	// version := integer '.' integer
	let version = do seq3(decimal_number(), ".".lit(), decimal_number())
		|major, _a2, minor| {result::Ok((major, minor))};
		
	// method := identifier lws url lws 'HTTP/' version crnl
	let method = do seq7(identifier(), lws, url, lws, "HTTP/".lit(), version, crnl)
		|name, _a2, url, _a4, _a5, version, _a7| {result::Ok((name, url, version))};
		
	// value := [^\r\n]+
	// continuation := crnl [ \t] value
	let value = match1({|c| c != '\r' && c != '\n'});
	let continuation = do seq3(crnl, ws, value)
		|_a1, _a2, v| {result::Ok(~" " + str::trim(*v))};
	
	// name := [^:]+
	// header := name ': ' value continuation* crnl
	// headers := header*
	let name = match1({|c| c != ':'});
	let header = do seq5(name, ":".lit(), value, continuation.r0(), crnl)
		|n, _a2, v, cnt, _a5| {result::Ok((str::to_lower(*n), str::trim(*v) + str::connect(*cnt, ~"")))};	// 4.2 says that header names are case-insensitive so we lower case them
	let headers = header.r0();
	
	// request := method headers crnl
	let request = do seq3(method, headers, crnl)
		|a1, h, _a2|
		{
			let (n, u, (v1, v2)) = a1;
			result::Ok(HttpRequest {method: *n, major_version: v1, minor_version: v2, url: decode(*u), headers: *h, body: ~""})};
	
	return request;
}

// We return a closure so that we can build the parser just once.
fn make_parser() -> fn@ (~str) -> result::Result<HttpRequest, ~str>
{
	|request: ~str|
	{
		let parser = request_parser();
		do result::chain_err(parser.parse(@~"http request", request))
		|err|
		{
			result::Err(fmt!("Expected %s on line %? col %?", *err.mesg, err.line, err.col))
		}
	}
}

#[cfg(test)]
fn equal<T: copy>(result: T, expected: T) -> bool
{
	if result != expected
	{
		io::stderr().write_line(fmt!("Expected %? but found %?", expected, result));
		return false;
	}
	return true;
}

#[test]
fn test_get_method1()
{
	let p = make_parser();
	
	match p(~"GET / HTTP/1.1\r\n\r\n")
	{
		result::Ok(value) =>
		{
			assert equal(value.method, ~"GET");
			assert equal(value.major_version, 1);
			assert equal(value.minor_version, 1);
			assert equal(value.url, ~"/");
			assert equal(value.headers.len(), 0u);
		}
		result::Err(mesg) =>
		{
			io::stderr().write_line(mesg);
			assert false;
		}
	}
}

#[test]
fn test_get_method2()
{
	let p = make_parser();
	
	match p(~"GET / HTTP/1.1\r\nHost: localhost:8080\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10.7; rv:11.0) Gecko/20100101 Firefox/11.0\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8\r\nAccept-Language: en-us,en;q=0.5\r\nAccept-Encoding: gzip, deflate\r\nConnection: keep-alive\r\n\r\n")
	{
		result::Ok(value) =>
		{
			assert equal(value.method, ~"GET");
			assert equal(value.major_version, 1);
			assert equal(value.minor_version, 1);
			assert equal(value.url, ~"/");
			assert equal(value.headers.len(), 6u);
			
			assert equal(value.headers.get(~"host"), ~"localhost:8080");
			assert equal(value.headers.get(~"user-agent"), ~"Mozilla/5.0 (Macintosh; Intel Mac OS X 10.7; rv:11.0) Gecko/20100101 Firefox/11.0");
			assert equal(value.headers.get(~"accept"), ~"text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8");
			assert equal(value.headers.get(~"accept-language"), ~"en-us,en;q=0.5");
			assert equal(value.headers.get(~"accept-encoding"), ~"gzip, deflate");
			assert equal(value.headers.get(~"connection"), ~"keep-alive");
		}
		result::Err(mesg) =>
		{
			io::stderr().write_line(mesg);
			assert false;
		}
	}
}

#[test]
fn test_unknown_method()
{
	let p = make_parser();
	
	match p(~"GET / HXTP/1.1\r\n\r\n")
	{
		result::Ok(value) =>
		{
			io::stderr().write_line(fmt!("Somehow parsed %?", value));
			assert false;
		}
		result::Err(mesg) =>
		{
			assert equal(mesg, ~"Expected 'HTTP/' on line 1 col 8");
		}
	}
}

#[test]
fn test_header_values()
{
	let p = make_parser();
	
	match p(~"GET / HTTP/1.1\r\nHost:   \t xxx\r\nBlah:   \t bbb \t\r\nMulti: line1\r\n  \tline2\r\n  line3\r\n\r\n")
	{
		result::Ok(value) =>
		{
			assert equal(value.headers.get(~"host"), ~"xxx");
			assert equal(value.headers.get(~"blah"), ~"bbb");
			assert equal(value.headers.get(~"multi"), ~"line1 line2 line3");
		}
		result::Err(mesg) =>
		{
			io::stderr().write_line(mesg);
			assert false;
		}
	}
}

#[test]
fn test_extension_method()
{
	let p = make_parser();
	
	match p(~"Explode \t / HTTP/1.1\r\nHost: xxx\r\n\r\nsome text\nand more text")
	{
		result::Ok(value) =>
		{
			assert equal(value.method, ~"Explode");
		}
		result::Err(mesg) =>
		{
			io::stderr().write_line(mesg);
			assert false;
		}
	}
}

#[test]
fn test_encoded_url()
{
	let p = make_parser();
	
	match p(~"GET /path%20with%20spaces HTTP/1.1\r\n\r\n")
	{
		result::Ok(value) =>
		{
			assert equal(value.url, ~"/path with spaces");
		}
		result::Err(mesg) =>
		{
			io::stderr().write_line(mesg);
			assert false;
		}
	}
}

#[test]
fn test_encoded_url2()
{
	let p = make_parser();
	
	match p(~"GET /path%2099with%20digits HTTP/1.1\r\n\r\n")
	{
		result::Ok(value) =>
		{
			assert equal(value.url, ~"/path 99with digits");
		}
		result::Err(mesg) =>
		{
			io::stderr().write_line(mesg);
			assert false;
		}
	}
}
