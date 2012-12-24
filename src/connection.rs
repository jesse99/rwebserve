//! The module responsible for communication using a persistent connection to a client.
//use socket::*;
use core::path::{GenericPath};
use core::send_map::linear::{LinearMap};
use request::{process_request, make_header_and_body};

// TODO: probably want to use task::unsupervise
pub fn handle_connection(config: &Config, fd: libc::c_int, local_addr: &str, remote_addr: &str)
{
	let request_port = oldcomm::Port();
	let request_chan = oldcomm::Chan(&request_port);
	let sse_port = oldcomm::Port();
	let sse_chan = oldcomm::Chan(&sse_port);
	let sock = @socket::socket::socket_handle(fd);
	
	// read_requests needs to run on its own thread so it doesn't block this task. 
	let ra = remote_addr.to_owned();
	do task::spawn_sched(task::SingleThreaded) {read_requests(ra, fd, request_chan);}
	
	let mut sse_tasks = LinearMap();
	loop
	{
		debug!("-----------------------------------------------------------");
		match oldcomm::select2(request_port, sse_port)
		{
			either::Left(option::Some(move request)) =>
			{
				let (header, body) = process_request(config, &mut sse_tasks, sse_chan, request, local_addr, remote_addr);
				write_response(sock, header, body);
			}
			either::Left(option::None) =>
			{
				close_sses(&sse_tasks);
				break;
			}
			either::Right(move body) =>
			{
				let response = make_response(config);
				let (_, body) = make_header_and_body(&response, StringBody(@body));
				write_response(sock, ~"", body);
			}
		}
	}
}

priv fn read_requests(remote_addr: &str, fd: libc::c_int, poke: oldcomm::Chan<option::Option<http_parser::HttpRequest>>)
{
	let sock = @socket::socket::socket_handle(fd);		// socket::socket_handle(fd);
	let parse = http_parser::make_parser();
	loop
	{
		let mut ok = false;
		let headers = read_headers(remote_addr, sock);
		if str::is_not_empty(headers)
		{
			match parse(headers)
			{
				result::Ok(ref request) =>
				{
					if request.headers.contains_key(~"content-length")
					{
						let body = read_body(sock, request.headers.get(~"content-length"));
						if str::is_not_empty(body)
						{
							oldcomm::send(poke, option::Some(http_parser::HttpRequest {body: body, ..copy *request}));
							ok = true;
						}
					}
					else
					{
						oldcomm::send(poke, option::Some(copy *request));
						ok = true;
					}
				}
				result::Err(ref mesg) =>
				{
					error!("Couldn't parse: '%s' from %s", *mesg, remote_addr);
					error!("%s", headers);
				}
			}
		}
		if !ok
		{
			// Client closed connection or there was some sort of error
			// (in which case the client will re-open a connection).
			info!("detached from %s", remote_addr);
			oldcomm::send(poke, option::None);
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
priv fn read_headers(remote_addr: &str, sock: @socket::socket::socket_handle) -> ~str unsafe
{
	let mut buffer = ~[];
	
	while !found_headers(buffer) 
	{
		match socket::socket::recv(sock, 1u)			// TODO: need a timeout
		{
			result::Ok(ref result) =>
			{
				if result.bytes > 0
				{
					vec::push(&mut buffer, result.buffer[0]);
				}
				else
				{
					// peer has closed its side of the connection
					return ~"";
				}
			}
			result::Err(ref mesg) =>
			{
				warn!("read_headers for %s failed with error: %s", remote_addr, *mesg);
				return ~"";
			}
		}
	}
	vec::push(&mut buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let c_str = cast::reinterpret_cast(&vec::raw::to_ptr(buffer));
		let headers = str::raw::from_c_str(c_str);
		debug!("headers: %s", headers);
		headers
	}
	else
	{
		error!("Headers were not utf-8");	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		~""
	}
}

priv fn found_headers(buffer: &[u8]) -> bool
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

priv fn read_body(sock: @socket::socket::socket_handle, content_length: ~str) -> ~str unsafe
{
	let total_len = option::get(uint::from_str(content_length));
	
	let mut buffer = ~[];
	vec::reserve(&mut buffer, total_len);
	
	while vec::len(buffer) < total_len 
	{
		match socket::socket::recv(sock, total_len - vec::len(buffer))			// TODO: need a timeout
		{
			result::Ok(ref result) =>
			{
				if result.bytes > 0
				{
					let mut i = 0u;
					while i < result.bytes
					{
						vec::push(&mut buffer, result.buffer[i]);
						i += 1u;
					}
				}
				else
				{
					// peer has closed its side of the connection
					return ~"";
				}
			}
			result::Err(ref mesg) =>
			{
				warn!("read_body failed with error: %s", *mesg);
				return ~"";
			}
		}
	}
	vec::push(&mut buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let body = str::raw::from_buf(vec::raw::to_ptr(buffer));
		debug!("body: %s", body);	// note that the log macros truncate really long strings 
		body
	}
	else
	{
		error!("Body was not utf-8");	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		~""
	}
}

// TODO: check connection: keep-alive
// TODO: presumbably when we switch to a better socket library we'll be able to handle errors here...
priv fn write_response(sock: @socket::socket::socket_handle, header: ~str, body: Body) unsafe
{
	fn write_body(sock: @socket::socket::socket_handle, body: &Body) unsafe
	{
		match *body
		{
			StringBody(text) =>
			{
				do str::as_buf(*text) |buffer, _len| 	{socket::socket::send_buf(sock, buffer, text.len())};
			}
			BinaryBody(binary) =>
			{
				socket::socket::send_buf(sock, vec::raw::to_ptr(*binary), binary.len());
			}
			CompoundBody(parts) =>
			{
				for parts.each |part| {write_body(sock, *part)};
			}
		}
	}
	
	do str::as_buf(header) |buffer, _len| {socket::socket::send_buf(sock, buffer, header.len())};
	write_body(sock, &body);
}

priv fn validate_config(config: &Config) -> ~str
{
	let mut errors = ~[];
	
	if vec::is_empty(config.hosts)
	{
		vec::push(&mut errors, ~"Hosts is empty.");
	}
	
	for vec::each(config.hosts)
	|host|
	{
		if str::is_empty(*host)
		{
			vec::push(&mut errors, ~"Host is empty.");
		}
	};
	
	if config.port < 1024_u16 && config.port != 80_u16
	{
		vec::push(&mut errors, ~"Port should be 80 or 1024 or above.");
	}
	
	if str::is_empty(config.server_info)
	{
		vec::push(&mut errors, ~"server_info is empty.");
	}
	
	if str::is_empty(config.resources_root.to_str())
	{
		vec::push(&mut errors, ~"resources_root is empty.");
	}
	else if !os::path_is_dir(&config.resources_root)
	{
		vec::push(&mut errors, ~"resources_root is not a directory.");
	}
	
	let mut names = ~[];
	for vec::each(~[~"forbidden.html", ~"home.html", ~"not-found.html", ~"not-supported.html"]) |name|
	{
		let path = config.resources_root.push(*name);
		if !os::path_exists(&path)
		{
			vec::push(&mut names, copy *name);
		}
	};
	if vec::is_not_empty(names)
	{
		vec::push(&mut errors, ~"Missing required files: " + str::connect(names, ~", "));
	}
	
	if str::is_empty(config.read_error)
	{
		vec::push(&mut errors, ~"read_error is empty.");
	}
	
	let mut missing_routes = ~[];
	let mut routes = ~[];
	for config.routes.each |entry|
	{
		if !config.views.contains_key(&entry.route)
		{
			vec::push(&mut missing_routes, copy entry.route);
		}
		vec::push(&mut routes, copy entry.route);
	};
	if vec::is_not_empty(missing_routes)
	{
		pure fn le(a: &~str, b: &~str) -> bool {*a <= *b}
		let missing_routes = std::sort::merge_sort(missing_routes, le);		// order depends on hash, but for unit tests we want to use something more consistent
		
		vec::push(&mut errors, fmt!("No views for the following routes: %s", str::connect(missing_routes, ~", ")));
	}
	
	let mut missing_views = ~[];
	for config.views.each_key |route|
	{
		if !vec::contains(routes, route)
		{
			vec::push(&mut missing_views, copy *route);
		}
	};
	if vec::is_not_empty(missing_views)
	{
		pure fn le(a: &~str, b: &~str) -> bool {*a <= *b}
		let missing_views = std::sort::merge_sort(missing_views, le);
		
		vec::push(&mut errors, fmt!("No routes for the following views: %s", str::connect(missing_views, ~", ")));
	}
	
	return str::connect(errors, ~" ");
}

#[test]
fn routes_must_have_views()
{
	let config = Config {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: GenericPath::from_str(~"server/html"),
		routes: ~[
			Route( ~"home", ~"GET", ~"/"),
			Route(~"greeting", ~"GET", ~"/hello"),
			Route(~"farewell", ~"GET", ~"/goodbye")],
		views: utils::linear_map_from_vector(~[(~"home",  missing_view)]),
		..initialize_config()};
		
	assert validate_config(&config) == ~"No views for the following routes: farewell, greeting";
}

#[test]
fn views_must_have_routes()
{
	let config = Config {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: GenericPath::from_str(~"server/html"),
		routes: ~[Route( ~"home", ~"GET", ~"/")],
		views: utils::linear_map_from_vector(~[(~"home",  missing_view), (~"greeting",  missing_view), (~"goodbye",  missing_view)]),
		..initialize_config()};
		
	assert validate_config(&config) == ~"No routes for the following views: goodbye, greeting";
}

#[test]
fn root_must_have_required_files()
{
	let config = Config {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: GenericPath::from_str(~"/tmp"),
		routes: ~[Route( ~"home", ~"GET", ~"/")],
		views: utils::linear_map_from_vector(~[(~"home",  missing_view)]),
		..initialize_config()};
		
	assert validate_config(&config) == ~"Missing required files: forbidden.html, home.html, not-found.html, not-supported.html";
}

