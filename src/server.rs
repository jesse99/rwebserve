import result = result::result;
import io;
import socket;
import std::map::hashmap; 
import http_parser::*;

export start, context_handler, config;

// ---- Exported Items --------------------------------------------------------
#[doc = "Function used to update a mustache context using an URL.

On entry the context includes:
* request-url: the url within the client request message (e.g. '/home').
* base-url: the url to the directory containing the template file.
* status-code: the code that will be included in the response message (e.g. '200' or '404').
* status-mesg: the code that will be included in the response message (e.g. 'OK' or 'Not Found')."]
type context_handler = fn~ (str, hashmap<str, mustache::data>) -> ();

#[doc = "Function used map a request url to a private url (or 'not-found' for bad requests)."]
type map_url_handler = fn~ (str) -> str;

#[doc = "Configuration information for the web server.

* server_info is included in the HTTP response and should include the server name and version.
* resources_root should be a path to where the files associated with URLs are loaded from.
* update_context is used to initialize a mustache context object based on the URL being serviced.
* host is the ip address (or 'localhost') that the server binds to.
* port is the TCP port that the server listens on."]
type config = {
	server_info: str,
	resources_root: str,
	url_mapper: map_url_handler,
	update_context: context_handler,
	host: str,
	port: u16};

#[doc = "Startup the server.

Currently this will run until a client does a get on '/shutdown' in which case exit is called."]
fn start(config: config)
{
	let r = result::chain(socket::bind_socket(config.host, config.port))
	{|shandle|
		result::chain(socket::listen(shandle, 10i32))
			{|shandle| attach(config, service_get, shandle)}
	};
	if result::is_failure(r)
	{
		#error["Couldn't start web server: %s", result::get_err(r)];
	}
}

// ---- Internal Items --------------------------------------------------------
const max_request_len:uint = 2048u;		// TODO: the standard says that there is no upper bound on these…

type payload = {template: str, mime_type: str, base_url: str};

type response = {body: str, mime_type: str, status_mesg: str};

type get_handler = fn~ (config, str, [str]) -> option<payload>;

// For now we'll let the OS handle caching of frequently used files (this
// also makes it easier to edit the html while the server is running).
fn service_get(config: config, url: str, types: [str]) -> option<payload>
{
	if url == "/shutdown"		// TODO: enable this via debug cfg (or maybe via a command line option)
	{
		#info["received shutdown request"];
		libc::exit(0_i32)
	}
	else
	{
		let extensions = std::map::hash_from_strs([("text/html", ".html"), ("text/css", ".css")]);		// TODO: should handle some other formats
		let path = if url == "/" {path::connect(config.resources_root, "home")} else {path::connect(config.resources_root, url)};
		for vec::each(types)
		{|mime_type|
			alt extensions.find(mime_type)
			{
				option::some(extension)
				{
					let path = if path.ends_with(extension) {path} else {path + extension};
					alt io::read_whole_file_str(path)
					{
						result::ok(contents)
						{
							let base_dir = path::dirname(url);
							let base_url = #fmt["http://%s:%?/%s/", config.host, config.port, base_dir];
							ret option::some({template: contents, mime_type: mime_type, base_url: base_url});
						}
						result::err(mesg)
						{
							#warn["%s", mesg];
						}
					}
				}
				option::none
				{
				}
			}
		}
		
		#error["Can't satisfy a request for %s with types: %s", url, str::connect(types, ", ")];
		option::none
	}
}

fn get_resource(config: config, get_fn: get_handler, request_url: str, url: str, types: [str], status_code: str, status_mesg: str) -> option<response>
{
	option::chain(get_fn(config, url, types))
	{|payload|
		let context = std::map::str_hash();
		context.insert("request-url", mustache::str(request_url));
		context.insert("base-url", mustache::str(payload.base_url));
		context.insert("status-code", mustache::str(status_code));
		context.insert("status-mesg", mustache::str(status_mesg));
		config.update_context(url, context);
		
		let rendered = mustache::render_str(payload.template, context);
		option::some({body: rendered, mime_type: payload.mime_type, status_mesg: status_code + " " + status_mesg})
	}
}

fn get_err_content(config: config, get_fn: get_handler, request_url: str, url: str, types: [str], status_code: str, status_mesg: str) -> response
{
	alt get_resource(config, get_fn, request_url, url, types + ["text/html"], status_code, status_mesg)
	{
		option::some(result)
		{
			result
		}
		option::none
		{
			fail
		}
	}
}

fn get_body(config: config, get_fn: get_handler, request_url: str, types: [str]) -> response
{
	// Don't allow clients to get resources above the root.
	if str::contains(request_url, "..")
	{
		get_err_content(config, get_fn, request_url, "forbidden", types, "403", "Forbidden")
	}
	else
	{
		let url = config.url_mapper(request_url);
		alt get_resource(config, get_fn, request_url, url, types, "200", "OK")
		{
			option::some(result)
			{
				result
			}
			option::none
			{
				get_err_content(config, get_fn, url, "not-found", types, "404", "Not Found")
			}
		}
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
// check connection: keep-alive
fn service_request(config: config, get_fn: get_handler, sock: @socket::socket_handle, request: http_request)
{
	if request.major_version == 1 && request.minor_version >= 1
	{
		#info["Servicing GET for %s", request.url];
		
		let types = if request.headers.contains_key("Accept") {str::split_char(request.headers.get("Accept"), ',')} else {["text/html"]};
		
		let response = get_body(config, get_fn, request.url, types);
		let header = #fmt["HTTP/1.1 %s\r\nServer: %s\r\nContent-Length: %?\r\nContent-Type: %s; charset=UTF-8\r\n\r\n", 
			response.status_mesg,
			config.server_info,
			str::len(response.body),
			response.mime_type];
		let trailer = "r\n\r\n";
		#debug["response header: %s", header];
		#debug["response body: %s", response.body];
		str::as_buf(header) 			{|buffer| socket::send_buf(sock, buffer, str::len(header))};
		str::as_buf(response.body)	{|buffer| socket::send_buf(sock, buffer, str::len(response.body))};
		str::as_buf(trailer)  			{|buffer| socket::send_buf(sock, buffer, str::len(trailer))};
	}
	else
	{
		#error["Only HTTP 1.x is supported (and x must be greater than 0)"];
	}
}

// TODO: probably want to use task::unsupervise
fn handle_client(config: config, get_fn: get_handler, fd: libc::c_int)
{
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
					service_request(config, get_fn, sock, request);
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

fn attach(config: config, get_fn: get_handler, shandle: @socket::socket_handle) -> result<@socket::socket_handle, str>
{
	#info["server is listening"];
	result::chain(socket::accept(shandle))
	{|fd|
		#info["attached to client"];
		task::spawn {|| handle_client(config, get_fn, fd)};
		result::ok(shandle)
	};
	attach(config, get_fn, shandle)
}

