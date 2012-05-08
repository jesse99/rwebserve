import option = option::option;
import result = result::result;
import io;
import socket;
import std::map::hashmap; 
import http_parser::*;

export request, response, response_handler, config, initialize_config, start;

// ---- Exported Items --------------------------------------------------------
#[doc = "Configuration information for the web server.

* host is the ip address (or 'localhost') that the server binds to.
* port is the TCP port that the server listens on.
* server_info is included in the HTTP response and should include the server name and version.
* resources_root should be a path to where the files associated with URLs are loaded from.
* routes: maps URI templates (\"/hello/{name}\") to route names (\"greeting\"). To support non text/html types append the template with \"<some/type>\".
* views: maps route names to view handler functions.
* static: used to handle URIs that don't match routes, but are found beneath resources_root.
* missing: used to handle URIs that don't match routes, and are not found beneath resources_root.
* static_types: maps file extensions (including the period) to mime types.
* read_error2: html used when a file fails to load. Must include {{request-url}} and {{path}} templates.
* read_error1: html used when a file fails to load. Must include {{request-url}} template.

initialize_config can be used to initialize some of these fields.
"]
type config = {
	host: str,
	port: u16,
	server_info: str,
	resources_root: str,
	routes: [(str, str)],					// better to use hashmap, but afaict there is no way to send a hashmap to a task
	views: [(str, response_handler)],
	static: response_handler,
	missing: response_handler,
	static_types: [(str, str)],
	read_error2: str,
	read_error1: str};
	
#[doc = "Information about incoming http requests. Passed into view functions.

* method: \"GET\", \"PUSH\", \"POST\", etc.
* request_url: path component of the URL.
* matches: contains entries from request_url matching a routes URI template.
* headers: headers from the http request.
* body: body of the http request."]
type request = {
	method: str,
	url: str,
	matches: hashmap<str, str>,
	headers: hashmap<str, str>,
	body: str};

#[doc = "Returned by view functions and used to generate http response messages.

* status: the status code and message for the response.
* headers: the HTTP headers to be included in the response.
* template: path relative to resources_root containing a template file.
* context: hashmap used when rendering the template file."]
type response = {
	status: str,
	headers: hashmap<str, str>,
	template: str,
	context: hashmap<str, mustache::data>};

#[doc = "Function used to generate an HTTP response.

On entry status will typically be set to \"200 OK\". Headers will include something like the following:
* Server: whizbang server 1.0
* Content-Length: 0 (if non-zero rwebserve will not compute the body length)
* Content-Type:  text/html; charset=UTF-8
Context will be initialized with:
* request-url: the url within the client request message (e.g. '/home').
* status-code: the code that will be included in the response message (e.g. '200' or '404').
* status-mesg: the code that will be included in the response message (e.g. 'OK' or 'Not Found').

On exit:
* status: is normally left unchanged.
* headers: existing headers may be modified and new ones added (e.g. to control caching).
* matches: should not be changed.
* template: should be set to a path relative to resources_root.
* context: new entries will often be added. If template is not actually a template file empty the context.

After the function returns a base-url entry is added to the context with the url to the directory containing the template file."]
type response_handler = fn~ (request, response) -> response;

#[doc = "Initalizes several config fields.

* port is initialized to 80.
* static is initialized to a reasonable view handler.
* missing is initialized to a view that assume a \"not-found.html\" is at the root.
* static_types is given entries for audio, image, video, and text extensions.
* read_error2 is initialized to a reasonable English language html error message.
* read_error1 is initialized to a reasonable English language html error message.
"]
fn initialize_config() -> config
{
	{
	host: "",
	port: 80_u16,
	server_info: "",
	resources_root: "",
	routes: [],
	views: [],
	static: static_view,
	missing: missing_view,
	static_types: [
		(".m4a", "audio/mp4"),
		(".m4b", "audio/mp4"),
		(".mp3", "audio/mpeg"),
		(".wav", "audio/vnd.wave"),
		
		(".gif", "image/gif"),
		(".jpeg", "image/jpeg"),
		(".jpg", "image/jpeg"),
		(".png", "image/png"),
		(".tiff", "image/tiff"),
		
		(".css", "text/css"),
		(".csv", "text/csv"),
		(".html", "text/html"),
		(".htm", "text/html"),
		(".txt", "text/plain"),
		(".text", "text/plain"),
		(".xml", "text/xml"),
		
		(".mp4", "video/mp4"),
		(".mov", "video/quicktime"),
		(".mpg", "video/mpeg"),
		(".mpeg", "video/mpeg"),
		(".qt", "video/quicktime")],
	read_error2: "<!DOCTYPE html>
<meta charset=utf-8>

<title>Error 403 (Forbidden)!</title>

<p>Failed to process URL {{request-url} (could not read {{path}}).</p>",
	read_error1: "<!DOCTYPE html>
<meta charset=utf-8>

<title>Error 403 (Forbidden)!</title>

<p>Could not read URL {{request-url}}.</p>"}
}

#[doc = "Startup the server.

Currently this will run until a client does a get on '/shutdown' in which case exit is called."]
fn start(config: config)
{
	let r = result::chain(socket::bind_socket(config.host, config.port))
	{|shandle|
		result::chain(socket::listen(shandle, 10i32))
			{|shandle| attach(config, shandle)}
	};
	if result::is_failure(r)
	{
		#error["Couldn't start web server: %s", result::get_err(r)];
	}
}

// ---- Internal Items --------------------------------------------------------
const max_request_len:uint = 2048u;		// TODO: the standard says that there is no upper bound on these…

// Task specific version of config. Should be identical to config (except that it uses
// hashmaps instead of arrays of tuples).
type internal_config = {
	host: str,
	port: u16,
	server_info: str,
	resources_root: str,
	routes_table: hashmap<str, str>,
	views_table: hashmap<str, response_handler>,
	static: response_handler,
	missing: response_handler,
	static_type_table: hashmap<str, str>,
	read_error2: str,
	read_error1: str};

// Default config.static view handler.
fn static_view(_request: request, response: response) -> response
{
	let path = mustache::render_str("{{request-url}}", response.context);
	{template: path, context: std::map::str_hash() with response}
}

// Default config.missing handler. Assumes that there is a "not-found.html"
// file at the resource root.
fn missing_view(_request: request, response: response) -> response
{
	{template: "not-found.html" with response}
}

fn validate_config(config: internal_config) -> str
{
	let mut errors = [];
	
	if str::is_empty(config.host)
	{
		vec::push(errors, "Host is empty.");
	}
	
	if config.port < 1024_u16 && config.port != 80_u16
	{
		vec::push(errors, "Port should be 80 or 1024 or above.");
	}
	
	if str::is_empty(config.server_info)
	{
		vec::push(errors, "server_info is empty.");
	}
	
	if str::is_empty(config.resources_root)
	{
		vec::push(errors, "resources_root is empty.");
	}
	else if !os::path_is_dir(config.resources_root)
	{
		vec::push(errors, "resources_root is not a directory.");
	}
	
	if str::is_empty(config.read_error2)
	{
		vec::push(errors, "read_error2 is empty.");
	}
	
	if str::is_empty(config.read_error1)
	{
		vec::push(errors, "read_error1 is empty.");
	}
	
	let mut missing_routes = [];
	let mut routes = [];
	for config.routes_table.each_value()
	{|route|
		if !config.views_table.contains_key(route)
		{
			vec::push(missing_routes, route);
		}
		vec::push(routes, route);
	};
	if vec::is_not_empty(missing_routes)
	{
		fn le(&&a: str, &&b: str) -> bool {a <= b}
		let missing_routes = std::sort::merge_sort(le, missing_routes);		// order depends on hash, but for unit tests we want to use something more consistent
		
		vec::push(errors, #fmt["No views for the following routes: %s", str::connect(missing_routes, ", ")]);
	}
	
	let mut missing_views = [];
	for config.views_table.each_key()
	{|route|
		if !vec::contains(routes, route)
		{
			vec::push(missing_views, route);
		}
	};
	if vec::is_not_empty(missing_views)
	{
		fn le(&&a: str, &&b: str) -> bool {a <= b}
		let missing_views = std::sort::merge_sort(le, missing_views);
		
		vec::push(errors, #fmt["No routes for the following views: %s", str::connect(missing_views, ", ")]);
	}
	
	ret str::connect(errors, " ");
}

fn get_body(config: internal_config, request: request, types: [str]) -> (response, str)
{
	if request.url == "/shutdown"		// TODO: enable this via debug cfg (or maybe via a command line option)
	{
		#info["received shutdown request"];
		libc::exit(0_i32)
	}
	else
	{
		let (status_code, status_mesg, mime_type, handler) = find_handler(config, request.url, types);
		
		let response = make_initial_response(config, status_code, status_mesg, mime_type, request.url);
		let response = handler(request, response);
		assert str::is_not_empty(response.template);
		
		process_template(config, response, request.url)
	}
}

fn find_handler(config: internal_config, request_url: str, types: [str]) -> (str, str, str, response_handler)
{
	let mut handler = option::none;
	let mut status_code = "200";
	let mut status_mesg = "OK";
	let mut result_type = "text/html; charset=UTF-8";
	
	// Try to find (an implicit) text/html handler.
	if vec::contains(types, "text/html")
	{
		handler = option::chain(config.routes_table.find(request_url))
			{|route| option::some(config.views_table.get(route))};
	}
	
	// Try to find a handler using an explicit mime type.
	if option::is_none(handler)
	{
		for vec::each(types)
		{|mime_type|
			let candidate = #fmt["%s<%s>", request_url, mime_type];
			
			let route = config.routes_table.find(candidate);
			if option::is_some(route)
			{
				handler = option::some(config.views_table.get(option::get(route)));
				result_type = mime_type + "; charset=UTF-8";
				break;
			}
		}
	}
	
	// See if the url matches a file under the resource root.
	if option::is_none(handler)
	{
		let path = path::normalize(path::connect(config.resources_root, request_url));
		if path.starts_with(config.resources_root)
		{
			if os::path_exists(path)
			{
				result_type = path_to_type(config, request_url);
				handler = option::some(config.static);
			}
		}
		else
		{
			status_code = "403";			// don't allow access to files not under resources_root
			status_mesg = "Forbidden";
			let (_, _, _, h) = find_handler(config, "forbidden", ["types/html"]);
			handler = option::some(h);
		}
	}
	
	// Otherwise use the missing handler.
	if option::is_none(handler)
	{
		status_code = "404";
		status_mesg = "Not Found";
		handler = option::some(config.missing);
	}
	
	ret (status_code, status_mesg, result_type, option::get(handler));
}

fn make_initial_response(config: internal_config, status_code: str, status_mesg: str, mime_type: str, request_url: str) -> response
{
	let headers = std::map::hash_from_strs([
		("Server", config.server_info),
		("Content-Length", "0"),
		("Content-Type", mime_type)]);
	
	let context = std::map::str_hash();
	context.insert("request-url", mustache::str(request_url));
	context.insert("status-code", mustache::str(status_code));
	context.insert("status-mesg", mustache::str(status_mesg));
	
	{status: status_code + " " + status_mesg, headers: headers, template: "", context: context}
}

fn process_template(config: internal_config, response: response, request_url: str) -> (response, str)
{
	let path = path::connect(config.resources_root, response.template);
	let (response, body) =
		alt io::read_whole_file_str(path)
		{
			result::ok(v)
			{
				(response, v)
			}
			result::err(mesg)
			{
				let context = std::map::str_hash();
				context.insert("request-url", response.context.get("request-url"));
				let body =
					if mustache::str(response.template) != response.context.get("request-url")
					{
						// We hard-code the body to ensure that we can always return something.
						context.insert("path", mustache::str(response.template));
						mustache::render_str(config.read_error2, context)
					}
					else
					{
						mustache::render_str(config.read_error1, context)
					};
				#error["Error '%s' tying to read '%s'", mesg, path];
				(make_initial_response(config, "403", "Forbidden", "text/html; charset=UTF-8", request_url), body)
			}
		};
	
	if !str::starts_with(response.status, "403") && response.context.size() > 0u
	{
		let base_dir = path::dirname(response.template);
		let base_url = #fmt["http://%s:%?/%s/", config.host, config.port, base_dir];
		response.context.insert("base-url", mustache::str(base_url));
		
		(response, mustache::render_str(body, response.context))
	}
	else
	{
		(response, body)
	}
}

fn path_to_type(config: internal_config, path: str) -> str
{
	let extension = tuple::second(path::splitext(path));
	if str::is_not_empty(extension)
	{
		alt config.static_type_table.find(extension)
		{
			option::some(v)
			{
				v + "; charset=UTF-8"
			}
			option::none
			{
				#warn["Couldn't find a static_types entry for %s", path];
				"text/html; charset=UTF-8"
			}
		}
	}
	else
	{
		#warn["Can't determine mime type for %s", path];
		"text/html; charset=UTF-8"
	}
}

fn recv_request(sock: @socket::socket_handle) -> str unsafe
{
	alt socket::recv(sock, max_request_len)
	{
		result::ok((buffer, len))
		{
			if str::is_utf8(buffer)
			{
				let request = str::unsafe::from_buf(vec::unsafe::to_ptr(buffer));
				#debug["request: %s", request];
				request
			}
			else
			{
				#error["Payload was not utf-8"];	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
				""
			}
		}
		result::err(mesg)
		{
			#warn["recv failed with error: %s", mesg];
			""
		}
	}
}

// TODO:
// should add date header (which must adhere to rfc1123)
// include last-modified and maybe etag
fn process_request(config: internal_config, request: http_request) -> (str, str)
{
	#info["Servicing GET for %s", request.url];
	
	let request = {method: "GET", url: request.url, matches: std::map::str_hash(), headers: request.headers, body: request.body};
	let types = if request.headers.contains_key("Accept") {str::split_char(request.headers.get("Accept"), ',')} else {["text/html"]};
	let (response, body) = get_body(config, request, types);
	
	let mut headers = "";
	for response.headers.each()
	{|name, value|
		if name == "Content-Length" && value == "0"
		{
			headers += #fmt["Content-Length: %?\r\n", str::len(body)];
		}
		else
		{
			headers += #fmt["%s: %s\r\n", name, value];
		}
	};
	
	let header = #fmt["HTTP/1.1 %s\r\n%s\r\n", response.status, headers];
	#debug["response header: %s", header];
	#debug["response body: %s", body];
	
	(header, body)
}

// TODO: check connection: keep-alive
fn service_request(config: internal_config, sock: @socket::socket_handle, request: http_request)
{
	if request.major_version == 1 && request.minor_version >= 1
	{
		let (header, body) = process_request(config, request);
		let trailer = "r\n\r\n";
		str::as_buf(header) 	{|buffer| socket::send_buf(sock, buffer, str::len(header))};
		str::as_buf(body)	{|buffer| socket::send_buf(sock, buffer, str::len(body))};
		str::as_buf(trailer)  	{|buffer| socket::send_buf(sock, buffer, str::len(trailer))};
	}
	else
	{
		#error["Only HTTP 1.x is supported (and x must be greater than 0)"];
	}
}

fn config_to_internal(config: config) -> internal_config
{
	{	host: config.host,
		port: config.port,
		server_info: config.server_info,
		resources_root: config.resources_root,
		routes_table: std::map::hash_from_strs(config.routes),
		views_table: std::map::hash_from_strs(config.views),
		static: config.static,
		missing: config.missing,
		static_type_table: std::map::hash_from_strs(config.static_types),
		read_error2: config.read_error2,
		read_error1: config.read_error1}
}

// TODO: probably want to use task::unsupervise
fn handle_client(config: config, fd: libc::c_int)
{
	let iconfig = config_to_internal(config);
	let err = validate_config(iconfig);
	if str::is_not_empty(err)
	{
		#error["Invalid config: %s", err];
		fail;
	}
	
	let sock = @socket::socket_handle(fd);
	let parse = make_parser();
	loop
	{
		let message = recv_request(sock);
		if str::is_not_empty(message)
		{
			alt parse(message)
			{
				result::ok(request)
				{
					service_request(iconfig, sock, request);
				}
				result::err(mesg)
				{
					#error["Couldn't parse: %s", mesg];
				}
			}
		}
		else
		{
			// Client closed connection or there was some sort of error
			// (in which case the client will re-open a connection).
			#info["detached from client"];
			break;
		}
	}
}

fn attach(config: config, shandle: @socket::socket_handle) -> result<@socket::socket_handle, str>
{
	#info["server is listening"];
	result::chain(socket::accept(shandle))
	{|fd|
		#info["attached to client"];
		task::spawn {|| handle_client(config, fd)};
		result::ok(shandle)
	};
	attach(config, shandle)
}

// ---- Unit Tests ------------------------------------------------------------
#[test]
fn routes_must_have_views()
{
	let config = {
		host: "localhost",
		server_info: "unit test",
		resources_root: "/tmp",
		routes: [("/", "home"), ("/hello", "greeting"), ("/goodbye", "farewell")],
		views: [("home",  missing_view)]
		with server::initialize_config()};
	let iconfig = config_to_internal(config);
	
	assert validate_config(iconfig) == "No views for the following routes: farewell, greeting";
}

#[test]
fn views_must_have_routes()
{
	let config = {
		host: "localhost",
		server_info: "unit test",
		resources_root: "/tmp",
		routes: [("/", "home")],
		views: [("home",  missing_view), ("greeting",  missing_view), ("goodbye",  missing_view)]
		with server::initialize_config()};
	let iconfig = config_to_internal(config);
	
	assert validate_config(iconfig) == "No routes for the following views: goodbye, greeting";
}
