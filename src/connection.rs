//! The module responsible for communication using a persistent connection to a client.
import socket;
import http_parser::*;
import request::{process_request, make_header_and_body};
import imap::{immutable_map, imap_methods};
import sse;

export handle_connection, conn_config, config_to_conn;

// Like config except that it is connection specific, uses hashmaps, and adds some fields for sse.
type conn_config = {
	hosts: ~[~str],
	port: u16,
	server_info: ~str,
	resources_root: ~str,
	route_list: ~[route],
	views_table: hashmap<~str, response_handler>,
	static: response_handler,
	sse_openers: hashmap<~str, open_sse>,	// key is a GET path
	sse_tasks: hashmap<~str, control_chan>,	// key is a GET path
	sse_push: comm::chan<~str>,
	missing: response_handler,
	static_type_table: hashmap<~str, ~str>,
	read_error: ~str,
	load_rsrc: rsrc_loader,
	valid_rsrc: rsrc_exists,
	settings: hashmap<~str, ~str>};

// TODO: probably want to use task::unsupervise
fn handle_connection(++config: config, fd: libc::c_int, local_addr: ~str, remote_addr: ~str)
{
	let sport = comm::port();
	let sch = comm::chan(sport);
	let eport = comm::port();
	let ech = comm::chan(eport);
	let sock = socket::create_socket(fd);			// @socket_handle(fd);
	
	let iconfig = config_to_conn(config, ech);
	let err = validate_config(iconfig);
	if str::is_not_empty(err)
	{
		#error["Invalid config: %s", err];
		fail;
	}
	
	do task::spawn {read_requests(remote_addr, fd, sch);}
	loop
	{
		#debug["-----------------------------------------------------------"];
		alt comm::select2(sport, eport)
		{
			either::left(option::some(request))
			{
				let (header, body) = process_request(iconfig, request, local_addr, remote_addr);
				write_response(sock, header, body);
			}
			either::left(option::none)
			{
				sse::close_sses(iconfig);
				break;
			}
			either::right(body)
			{
				let response = sse::make_response(iconfig);
				let (_, body) = make_header_and_body(response, body);
				write_response(sock, ~"", body);
			}
		}
	}
}

fn read_requests(remote_addr: ~str, fd: libc::c_int, poke: comm::chan<option::option<http_request>>)
{
	let sock = socket::create_socket(fd);		// socket::socket_handle(fd);
	let parse = make_parser();
	loop
	{
		let headers = read_headers(sock);
		if str::is_not_empty(headers)
		{
			alt parse(headers)
			{
				result::ok(request)
				{
					if request.headers.contains_key(~"content-length")
					{
						let body = read_body(sock, request.headers.get(~"content-length"));
						if str::is_not_empty(body)
						{
							comm::send(poke, option::some({body: body with request}));
						}
						else
						{
							#info["Ignoring %s and %s from %s", headers, utils::truncate_str(body, 80), remote_addr];
						}
					}
					else
					{
						comm::send(poke, option::some(request));
					}
				}
				result::err(mesg)
				{
					#error["Couldn't parse: '%s' from %s", mesg, remote_addr];
					#error["%s", headers];
				}
			}
		}
		else
		{
			// Client closed connection or there was some sort of error
			// (in which case the client will re-open a connection).
			#info["detached from %s", remote_addr];
			comm::send(poke, option::none);
			break;
		}
	}
}

// TODO: We can't simply do a read for whatever is available because
// clients can issue multple requests. So we need to read the request
// byte by byte until we get a double new-line. If this becomes a bottle
// neck we could do chunked reads, but we'd need to take care to properly
// handle multi-byte utf-8 characters and the split between headers and
// the body.
fn read_headers(sock: @socket::socket_handle) -> ~str unsafe
{
	let mut buffer = ~[];
	
	while !found_headers(buffer) 
	{
		alt socket::recv(sock, 1u)			// TODO: need a timeout
		{
			result::ok(result)
			{
				vec::push(buffer, result.buffer[0]);
			}
			result::err(mesg)
			{
				#warn["read_headers failed with error: %s", mesg];
				ret ~"";
			}
		}
	}
	vec::push(buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let mut headers = str::unsafe::from_buf(vec::unsafe::to_ptr(buffer));
		str::unsafe::set_len(headers, vec::len(buffer));		// push adds garbage after the end of the actual elements (i.e. the capacity part)
		#debug["headers: %s", headers];
		headers
	}
	else
	{
		#error["Headers were not utf-8"];	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		~""
	}
}

fn found_headers(buffer: ~[u8]) -> bool
{
	if vec::len(buffer) < 4u
	{
		false
	}
	else
	{
		let len = vec::len(buffer);
		buffer[len-4u] == 0x0Du8 && buffer[len-3u] == 0x0Au8 && buffer[len-2u] == 0x0Du8 && buffer[len-1u] == 0x0Au8
	}
}

fn read_body(sock: @socket::socket_handle, content_length: ~str) -> ~str unsafe
{
	let total_len = option::get(uint::from_str(content_length));
	
	let mut buffer = ~[];
	vec::reserve(buffer, total_len);
	
	while vec::len(buffer) < total_len 
	{
		alt socket::recv(sock, total_len - vec::len(buffer))			// TODO: need a timeout
		{
			result::ok(result)
			{
				let mut i = 0u;
				while i < result.bytes
				{
					vec::push(buffer, result.buffer[i]);
					i += 1u;
				}
			}
			result::err(mesg)
			{
				#warn["read_body failed with error: %s", mesg];
				ret ~"";
			}
		}
	}
	vec::push(buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let body = str::unsafe::from_buf(vec::unsafe::to_ptr(buffer));
		#debug["body: %s", body];	// note that the log macros truncate really long strings 
		body
	}
	else
	{
		#error["Body was not utf-8"];	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		~""
	}
}

// TODO: check connection: keep-alive
// TODO: presumbably when we switch to a better socket library we'll be able to handle errors here...
fn write_response(sock: @socket::socket_handle, header: ~str, body: ~str)
{
	// It's probably more efficient to do the concatenation rather than two sends because
	// we'll avoid a context switch into the kernel. In any case this seems to increase the
	// likelihood of the network stack putting all of this into a single packet which makes
	// packets easier to analyze.
	let data = header + body;
	do str::as_buf(data) |buffer| {socket::send_buf(sock, buffer, str::len(data))};
}

fn config_to_conn(config: config, push: comm::chan<~str>) -> conn_config
{
	{	hosts: config.hosts,
		port: config.port,
		server_info: config.server_info,
		resources_root: config.resources_root,
		route_list: vec::map(config.routes, to_route),
		views_table: std::map::hash_from_strs(config.views),
		static: copy(config.static),
		sse_openers: std::map::hash_from_strs(config.sse),
		sse_tasks: std::map::str_hash(),
		sse_push: push,
		missing: copy(config.missing),
		static_type_table: std::map::hash_from_strs(config.static_types),
		read_error: config.read_error,
		load_rsrc: copy(config.load_rsrc),
		valid_rsrc: copy(config.valid_rsrc),
		settings: std::map::hash_from_strs(config.settings)}
}

fn validate_config(config: conn_config) -> ~str
{
	let mut errors = ~[];
	
	if vec::is_empty(config.hosts)
	{
		vec::push(errors, ~"Hosts is empty.");
	}
	
	for vec::each(config.hosts)
	|host|
	{
		if str::is_empty(host)
		{
			vec::push(errors, ~"Host is empty.");
		}
	};
	
	if config.port < 1024_u16 && config.port != 80_u16
	{
		vec::push(errors, ~"Port should be 80 or 1024 or above.");
	}
	
	if str::is_empty(config.server_info)
	{
		vec::push(errors, ~"server_info is empty.");
	}
	
	if str::is_empty(config.resources_root)
	{
		vec::push(errors, ~"resources_root is empty.");
	}
	else if !os::path_is_dir(config.resources_root)
	{
		vec::push(errors, ~"resources_root is not a directory.");
	}
	
	let mut names = ~[];
	for vec::each(~[~"forbidden.html", ~"home.html", ~"not-found.html", ~"not-supported.html"])
	|name|
	{
		let path = path::connect(config.resources_root, name);
		if !os::path_exists(path)
		{
			vec::push(names, name);
		}
	};
	if vec::is_not_empty(names)
	{
		vec::push(errors, ~"Missing required files: " + str::connect(names, ~", "));
	}
	
	if str::is_empty(config.read_error)
	{
		vec::push(errors, ~"read_error is empty.");
	}
	
	let mut missing_routes = ~[];
	let mut routes = ~[];
	for config.route_list.each()
	|entry|
	{
		let route = entry.route;
		if !config.views_table.contains_key(route)
		{
			vec::push(missing_routes, route);
		}
		vec::push(routes, route);
	};
	if vec::is_not_empty(missing_routes)
	{
		fn le(&&a: ~str, &&b: ~str) -> bool {a <= b}
		let missing_routes = std::sort::merge_sort(le, missing_routes);		// order depends on hash, but for unit tests we want to use something more consistent
		
		vec::push(errors, #fmt["No views for the following routes: %s", str::connect(missing_routes, ~", ")]);
	}
	
	let mut missing_views = ~[];
	for config.views_table.each_key()
	|route|
	{
		if !vec::contains(routes, route)
		{
			vec::push(missing_views, route);
		}
	};
	if vec::is_not_empty(missing_views)
	{
		fn le(&&a: ~str, &&b: ~str) -> bool {a <= b}
		let missing_views = std::sort::merge_sort(le, missing_views);
		
		vec::push(errors, #fmt["No routes for the following views: %s", str::connect(missing_views, ~", ")]);
	}
	
	ret str::connect(errors, ~" ");
}


fn to_route(input: (~str, ~str, ~str)) -> route
{
	alt input
	{
		(method, template_str, route)
		{
			let i = str::find_char(template_str, '<');
			let (template, mime_type) = if option::is_some(i)
				{
					let j = str::find_char_from(template_str, '>', option::get(i)+1u);
					assert option::is_some(j);
					(str::slice(template_str, 0u, option::get(i)), str::slice(template_str, option::get(i)+1u, option::get(j)))
				}
				else
				{
					(template_str, ~"text/html")
				};
			
			{method: method, template: uri_template::compile(template), mime_type: mime_type, route: route}
		}
	}
}

#[test]
fn routes_must_have_views()
{
	let config = {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: ~"server/html",
		routes: ~[(~"GET", ~"/", ~"home"), (~"GET", ~"/hello", ~"greeting"), (~"GET", ~"/goodbye", ~"farewell")],
		views: ~[(~"home",  missing_view)]
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	assert validate_config(iconfig) == ~"No views for the following routes: farewell, greeting";
}

#[test]
fn views_must_have_routes()
{
	let config = {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: ~"server/html",
		routes: ~[(~"GET", ~"/", ~"home")],
		views: ~[(~"home",  missing_view), (~"greeting",  missing_view), (~"goodbye",  missing_view)]
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	assert validate_config(iconfig) == ~"No routes for the following views: goodbye, greeting";
}

#[test]
fn root_must_have_required_files()
{
	let config = {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: ~"/tmp",
		routes: ~[(~"GET", ~"/", ~"home")],
		views: ~[(~"home",  missing_view)]
		with initialize_config()};
		
	let eport = comm::port();
	let ech = comm::chan(eport);
	let iconfig = config_to_conn(config, ech);
	
	assert validate_config(iconfig) == ~"Missing required files: forbidden.html, home.html, not-found.html, not-supported.html";
}

