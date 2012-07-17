/// Handles an incoming request from a client connection and sends a response.
import http_parser::*;
import imap::imap_methods;
import connection::*;
import sse::*;
import utils::*;

export process_request, make_header_and_body, make_initial_response;

// TODO:
// include last-modified and maybe etag
fn process_request(config: conn_config, request: http_request, local_addr: str, remote_addr: str) -> (str, str)
{
	#info["Servicing %s for %s", request.method, request.url];
	
	let version = #fmt["%d.%d", request.major_version, request.minor_version];
	let (path, params) = parse_url(request.url);
	let request = {version: version, method: request.method, local_addr: local_addr, remote_addr: remote_addr, path: path, matches: std::map::str_hash(), params: params, headers: std::map::hash_from_strs(request.headers), body: request.body};
	let types = if request.headers.contains_key("accept") {str::split_char(request.headers.get("accept"), ',')} else {["text/html"]/~};
	let (response, body) = get_body(config, request, types);
	
	let (header, body) = make_header_and_body(response, body);
	#debug["response header: %s", header];
	#debug["response body: %s", body];
	
	(header, body)
}

fn parse_url(url: str) -> (str, imap::imap<str, str>)
{
	alt str::find_char(url, '?')
	{
		option::some(i)
		{
			let query = str::slice(url, i+1, str::len(url));
			let parts = str::split_char(query, '&');
			let params = do vec::map(parts) |p| {str::split_char(p, '=')};
			if do vec::all(params) |p| {vec::len(p) == 2}
			{
				(str::slice(url, 0, i), do vec::map(params) |p| {(p[0], p[1])})
			}
			else
			{
				// It's not a valid query string so we'll just let the server handle it.
				// Presumbably it won't match any routes so we'll get an error then.
				(url, ~[])
			}
		}
		option::none
		{
			(url, ~[])
		}
	}
}

fn make_header_and_body(response: response, body: str) -> (str, str)
{
	let mut headers = "";
	let mut has_content_len = false;
	let mut is_chunked = false;
	
	for response.headers.each()
	|name, value|
	{
		if name == "Content-Length"
		{
			has_content_len = true;
		}
		else if name == "Transfer-Encoding" && value == "chunked"
		{
			is_chunked = true;
		}
		
		if name == "Content-Length" && value == "0"
		{
			headers += #fmt["Content-Length: %?\r\n", str::len(body)];
		}
		else
		{
			headers += #fmt["%s: %s\r\n", name, value];
		}
	};
	
	if is_chunked
	{
		assert !has_content_len;
	}
	else if !has_content_len
	{
		headers += #fmt["Content-Length: %?\r\n", str::len(body)];
	}
	
	(#fmt["HTTP/1.1 %s\r\n%s\r\n", response.status, headers],
		if is_chunked {#fmt["%X\r\n%s\r\n", str::len(body), body]} else {body})
}

fn get_body(config: conn_config, request: request, types: ~[str]) -> (response, str)
{
	if vec::contains(types, "text/event-stream")
	{
		process_sse(config, request)
	}
	else
	{
		let (status_code, status_mesg, mime_type, handler, matches) = find_handler(copy(config), request.method, request.path, types, request.version);
		
		let response = make_initial_response(config, status_code, status_mesg, mime_type, request);
		let response = handler(config.settings, {matches: matches with request}, response);
		
		if str::is_not_empty(response.template)
		{
			assert str::is_empty(response.body);
			
			process_template(config, response, request)
		}
		else
		{
			(response, response.body)
		}
	}
}

fn find_handler(+config: conn_config, method: str, request_path: str, types: ~[str], version: str) -> (str, str, str, response_handler, hashmap<str, str>)
{
	let mut handler = option::none;
	let mut status_code = "200";
	let mut status_mesg = "OK";
	let mut result_type = "text/html; charset=UTF-8";
	let mut matches = std::map::str_hash();
	
	// According to section 3.1 servers are supposed to accept new minor version editions.
	if !str::starts_with(version, "1.")
	{
		status_code = "505";
		status_mesg = "HTTP Version Not Supported";
		let (_, _, _, h, _) = find_handler(copy(config), method, "not-supported.html", ["types/html"]/~, "1.1");
		handler = option::some(h);
		#info["responding with %s %s", status_code, status_mesg];
	}
	
	// See if the url matches a file under the resource root (i.e. the url can't have too many .. components).
	if option::is_none(handler)
	{
		let path = path::normalize(path::connect(config.resources_root, request_path));
		if str::starts_with(path, config.resources_root)
		{
			if config.valid_rsrc(path)
			{
				let mime_type = path_to_type(config, request_path);
				if vec::contains(types, "*/*") || vec::contains(types, mime_type)
				{
					result_type = mime_type + "; charset=UTF-8";
					handler = option::some(copy(config.static));
				}
			}
		}
		else
		{
			status_code = "403";			// don't allow access to files not under resources_root
			status_mesg = "Forbidden";
			let (_, _, _, h, _) = find_handler(copy(config), method, "forbidden.html", ["types/html"]/~, version);
			handler = option::some(h);
			#info["responding with %s %s (path wasn't under resources_root)", status_code, status_mesg];
		}
	}
	
	// Then look for the first matching route.
	if option::is_none(handler)
	{
		for vec::each(config.route_list)
		|entry|
		{
			if entry.method == method
			{
				let m = uri_template::match(request_path, entry.template);
				if m.size() > 0u
				{
					if vec::contains(types, entry.mime_type)
					{
						handler = option::some(config.views_table.get(entry.route));
						result_type = entry.mime_type + "; charset=UTF-8";
						matches = m;
						break;
					}
					else
					{
						#info["request matches route but route type is %s not one of: %s", entry.mime_type, str::connect(types, ", ")];
					}
				}
			}
		}
	}
	
	// Otherwise use the missing handler.
	if option::is_none(handler)
	{
		status_code = "404";
		status_mesg = "Not Found";
		handler = option::some(copy(config.missing));
		#info["responding with %s %s", status_code, status_mesg];
	}
	
	ret (status_code, status_mesg, result_type, option::get(handler), matches);
}

fn make_initial_response(config: conn_config, status_code: str, status_mesg: str, mime_type: str, request: request) -> response
{
	let headers = std::map::hash_from_strs(~[
		("Content-Type", mime_type),
		("Date", std::time::now_utc().rfc822()),
		("Server", config.server_info),
	]);
	
	if config.settings.contains_key("debug") && config.settings.get("debug") == "true"
	{
		headers.insert("Cache-Control", "no-cache");
	}
	
	let context = std::map::str_hash();
	context.insert("request-path", mustache::str(@request.path));
	context.insert("status-code", mustache::str(@status_code));
	context.insert("status-mesg", mustache::str(@status_mesg));
	context.insert("request-version", mustache::str(@request.version));
	
	{status: status_code + " " + status_mesg, headers: headers, body: "", template: "", context: context}
}

fn load_template(config: conn_config, path: str) -> result::result<str, str>
{
	// {{ should be followed by }} (rust-mustache hangs if this is not the case).
	fn match_curly_braces(text: str) -> bool
	{
		let mut index = 0u;
		
		while index < str::len(text)
		{
			alt str::find_str_from(text, "{{", index)
			{
				option::some(i)
				{
					alt str::find_str_from(text, "}}", i + 2u)
					{
						option::some(j)
						{
							index = j + 2u;
						}
						option::none()
						{
							ret false;
						}
					}
				}
				option::none
				{
					break;
				}
			}
		}
		ret true;
	}
	
	do result::chain(config.load_rsrc(path))
	|template|
	{
		if !config.settings.contains_key("debug") || config.settings.get("debug") == "false" || match_curly_braces(template)
		{
			result::ok(template)
		}
		else
		{
			result::err("mismatched curly braces")
		}
	}
}

fn process_template(config: conn_config, response: response, request: request) -> (response, str)
{
	let path = path::connect(config.resources_root, response.template);
	let (response, body) =
		alt load_template(config, path)
		{
			result::ok(v)
			{
				// We found a legit template file.
				(response, v)
			}
			result::err(mesg)
			{
				// We failed to load the template so use the hard-coded config.read_error body.
				let context = std::map::str_hash();
				context.insert("request-path", mustache::str(@request.path));
				let body = mustache::render_str(config.read_error, context);
				
				if config.server_info != "unit test"
				{
					#error["Error '%s' tying to read '%s'", mesg, path];
				}
				(make_initial_response(config, "403", "Forbidden", "text/html; charset=UTF-8", request), body)
			}
		};
	
	if !str::starts_with(response.status, "403") && response.context.size() > 0u
	{
		// If we were able to load a template, and we have context, then use the
		// context to expand the template.
		let base_dir = path::dirname(response.template);
		let base_url = #fmt["http://%s:%?/%s/", request.local_addr, config.port, base_dir];
		response.context.insert("base-path", mustache::str(@base_url));
		
		(response, mustache::render_str(body, response.context))
	}
	else
	{
		(response, body)
	}
}

fn path_to_type(config: conn_config, path: str) -> str
{
	let extension = tuple::second(path::splitext(path));
	if str::is_not_empty(extension)
	{
		alt config.static_type_table.find(extension)
		{
			option::some(v)
			{
				v
			}
			option::none
			{
				#warn["Couldn't find a static_types entry for %s", path];
				"text/html"
			}
		}
	}
	else
	{
		#warn["Can't determine mime type for %s", path];
		"text/html"
	}
}

#[cfg(test)]
fn test_view(_settings: hashmap<str, str>, _request: request, response: response) -> response
{
	{template: "test.html" with response}
}

#[cfg(test)]
fn null_loader(path: str) -> result::result<str, str>
{
	result::ok(path + " contents")
}

#[cfg(test)]
fn err_loader(path: str) -> result::result<str, str>
{
	result::err(path + " failed to load")
}

#[cfg(test)]
fn make_request(url: str, mime_type: str) -> http_request
{
	let headers = ~[		// http_parser lower cases header names so we do too
		("host", "localhost:8080"),
		("user-agent", "Mozilla/5.0"),
		("accept", mime_type),
		("accept-Language", "en-us,en"),
		("accept-encoding", "gzip, deflate"),
		("connection", "keep-alive")];
	{method: "GET", major_version: 1, minor_version: 1, url: url, headers: headers, body: ""}
}

#[test]
fn html_route()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/bar", "text/html");
	let (_header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert body == "server/html/test.html contents";
}

#[test]
fn route_with_bad_type()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/bar", "text/zzz");
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("404 Not Found");
	assert header.contains("Content-Type: text/html");
	assert body == "server/html/not-found.html contents";
}

#[test]
fn non_html_route()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar<text/csv>", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/bar", "text/csv");
	let (_header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert body == "server/html/test.html contents";
}

#[test]
fn static_route()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader,
		valid_rsrc: |_path| {true}
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/baz.jpg", "text/html,image/jpeg");
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("Content-Type: image/jpeg");
	assert body == "server/html/foo/baz.jpg contents";
}

#[test]
fn static_with_bad_type()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader,
		valid_rsrc: |_path| {true}
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/baz.jpg", "text/zzz");
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("Content-Type: text/html");
	assert body == "server/html/not-found.html contents";
}

#[test]
fn bad_url()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader,
		valid_rsrc: |_path| {false}
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/baz.jpg", "text/html,image/jpeg");
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("Content-Type: text/html");
	assert header.contains("404 Not Found");
	assert str::contains(body, "server/html/not-found.html content");
}

#[test]
fn path_outside_root()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/bar", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader,
		valid_rsrc: |_path| {true}
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/../../baz.jpg", "text/html,image/jpeg");
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("Content-Type: text/html");
	assert header.contains("403 Forbidden");
	assert str::contains(body, "server/html/not-found.html contents");
}

#[test]
fn read_error()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/baz", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: err_loader,
		valid_rsrc: |_path| {true}
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = make_request("/foo/baz.jpg", "text/html,image/jpeg");
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("Content-Type: text/html");
	assert header.contains("403 Forbidden");
	assert str::contains(body, "Could not read URL /foo/baz.jpg");
}

#[test]
fn bad_version()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/baz", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: null_loader,
		valid_rsrc: |_path| {true}
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	let request = {major_version: 100 with make_request("/foo/baz.jpg", "text/html,image/jpeg")};
	let (header, body) = process_request(iconfig, request, "10.11.12.13", "1.2.3.4");
	
	assert header.contains("Content-Type: text/html");
	assert header.contains("505 HTTP Version Not Supported");
	assert str::contains(body, "server/html/not-found.html contents");
}

#[test]
fn bad_template()
{
	let loader: rsrc_loader = |_path| {result::ok("unbalanced {{curly}} {{braces}")};
	
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/foo/baz", "foo")]/~,
		views: [("foo",  test_view)]/~,
		load_rsrc: loader,
		valid_rsrc: |_path| {true},
		settings: [("debug", "true")]/~
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	alt load_template(iconfig, "blah.html")
	{
		result::ok(v)
		{
			io::stderr().write_line("Expected error but found: " + v);
			assert false;
		}
		result::err(s)
		{
			assert str::contains(s, "mismatched curly braces");
		}
	}
}

#[test]
fn query_strings()
{
	let (path, params) = parse_url("/some/url");
	assert check_strs(path, "/some/url");
	assert check_vectors(params, ~[]);
	
	let (path, params) = parse_url("/some/url?badness");
	assert check_strs(path, "/some/url?badness");
	assert check_vectors(params, ~[]);
	
	let (path, params) = parse_url("/some?name=value");
	assert check_strs(path, "/some");
	assert check_vectors(params, ~[("name", "value")]);
	
	let (path, params) = parse_url("/some?name=value&foo=bar");
	assert check_strs(path, "/some");
	assert check_vectors(params, ~[("name", "value"), ("foo", "bar")]);
}
