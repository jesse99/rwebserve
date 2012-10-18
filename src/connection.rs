//! The module responsible for communication using a persistent connection to a client.
//use socket::*;
//use http_parser::*;
use request::{process_request, make_header_and_body};

// Like config except that it is connection specific, uses hashmaps, and adds some fields for sse.
pub struct ConnConfig
{
	pub hosts: ~[~str],
	pub port: u16,
	pub server_info: ~str,
	pub resources_root: Path,
	pub route_list: ~[Route],
	pub views_table: HashMap<@~str, ResponseHandler>,
	pub static_handler: ResponseHandler,
	pub is_template: IsTemplateFile,
	pub sse_openers: HashMap<@~str, OpenSse>,		// key is a GET path
	pub sse_tasks: HashMap<@~str, ControlChan>,	// key is a GET path
	pub sse_push: comm::Chan<~str>,
	pub missing: ResponseHandler,
	pub static_type_table: HashMap<@~str, @~str>,
	pub read_error: ~str,
	pub load_rsrc: RsrcLoader,
	pub valid_rsrc: RsrcExists,
	pub settings: HashMap<@~str, @~str>,
	
	drop {}
}

pub fn config_to_conn(config: &Config, push: comm::Chan<~str>) -> ConnConfig
{
	ConnConfig {
		hosts: config.hosts,
		port: config.port,
		server_info: config.server_info,
		resources_root: config.resources_root,
		route_list: vec::map(config.routes, to_route),
		views_table: utils::boxed_hash_from_strs(config.views),
		static_handler: copy config.static_handler,
		is_template: copy config.is_template,
		sse_openers: utils::boxed_hash_from_strs(config.sse),
		sse_tasks: std::map::HashMap(),
		sse_push: push,
		missing: copy config.missing,
		static_type_table: utils::to_boxed_str_hash(config.static_types),
		read_error: config.read_error,
		load_rsrc: copy config.load_rsrc,
		valid_rsrc: copy config.valid_rsrc,
		settings: utils::to_boxed_str_hash(config.settings),
	}
}

// TODO: probably want to use task::unsupervise
pub fn handle_connection(config: &Config, fd: libc::c_int, local_addr: &str, remote_addr: &str)
{
	let request_port = comm::Port();
	let request_chan = comm::Chan(&request_port);
	let sse_port = comm::Port();
	let sse_chan = comm::Chan(&sse_port);
	let sock = @socket::socket::socket_handle(fd);
	
	let iconfig = config_to_conn(config, sse_chan);
	let err = validate_config(&iconfig);
	if str::is_not_empty(err)
	{
		error!("Invalid config: %s", err);
		fail;
	}
	
	// read_requests needs to run on its own thread so it doesn't block this task. 
	let ra = remote_addr.to_unique();
	do task::spawn_sched(task::SingleThreaded) {read_requests(ra, fd, request_chan);}
	
	loop
	{
		debug!("-----------------------------------------------------------");
		match comm::select2(request_port, sse_port)
		{
			either::Left(option::Some(ref request)) =>
			{
				let (header, body) = process_request(&iconfig, request, local_addr, remote_addr);
				write_response(sock, header, body);
			}
			either::Left(option::None) =>
			{
				close_sses(&iconfig);
				break;
			}
			either::Right(move body) =>
			{
				let response = make_response(&iconfig);
				let (_, body) = make_header_and_body(&response, StringBody(@body));
				write_response(sock, ~"", body);
			}
		}
	}
}

priv fn read_requests(remote_addr: &str, fd: libc::c_int, poke: comm::Chan<option::Option<http_parser::HttpRequest>>)
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
							comm::send(poke, option::Some(http_parser::HttpRequest {body: body, ..*request}));
							ok = true;
						}
					}
					else
					{
						comm::send(poke, option::Some(copy *request));
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
			comm::send(poke, option::None);
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
		let mut headers = str::raw::from_buf(vec::raw::to_ptr(buffer));
		str::raw::set_len(&mut headers, vec::len(buffer));		// push adds garbage after the end of the actual elements (i.e. the capacity part)
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
	let total_len = option::get(&uint::from_str(content_length));
	
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

priv fn validate_config(config: &ConnConfig) -> ~str
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
	for vec::each(~[~"forbidden.html", ~"home.html", ~"not-found.html", ~"not-supported.html"])
	|name|
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
	for config.route_list.each()
	|entry|
	{
		if !config.views_table.contains_key(@copy entry.route)
		{
			vec::push(&mut missing_routes, copy entry.route);
		}
		vec::push(&mut routes, copy entry.route);
	};
	if vec::is_not_empty(missing_routes)
	{
		pure fn le(a: &~str, b: &~str) -> bool {*a <= *b}
		let missing_routes = std::sort::merge_sort(le, missing_routes);		// order depends on hash, but for unit tests we want to use something more consistent
		
		vec::push(&mut errors, fmt!("No views for the following routes: %s", str::connect(missing_routes, ~", ")));
	}
	
	let mut missing_views = ~[];
	for config.views_table.each_key()
	|route|
	{
		if !vec::contains(routes, route)
		{
			vec::push(&mut missing_views, copy *route);
		}
	};
	if vec::is_not_empty(missing_views)
	{
		pure fn le(a: &~str, b: &~str) -> bool {*a <= *b}
		let missing_views = std::sort::merge_sort(le, missing_views);
		
		vec::push(&mut errors, fmt!("No routes for the following views: %s", str::connect(missing_views, ~", ")));
	}
	
	return str::connect(errors, ~" ");
}

pub fn to_route(input: &(~str, ~str, ~str)) -> Route
{
	match *input
	{
		(ref method, copy template_str, ref route) =>
		{
			let i = str::find_char(template_str, '<');
			let (template, mime_type) = if option::is_some(&i)
				{
					let j = str::find_char_from(template_str, '>', option::get(&i)+1u);
					assert option::is_some(&j);
					(str::slice(template_str, 0u, option::get(&i)), str::slice(template_str, option::get(&i)+1u, option::get(&j)))
				}
				else
				{
					(template_str, ~"text/html")
				};
			
			Route {method: *method, template: uri_template::compile(template), mime_type: mime_type, route: *route}
		}
	}
}

#[test]
fn routes_must_have_views()
{
	let config = Config {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: path::from_str(~"server/html"),
		routes: ~[(~"GET", ~"/", ~"home"), (~"GET", ~"/hello", ~"greeting"), (~"GET", ~"/goodbye", ~"farewell")],
		views: ~[(~"home",  missing_view)],
		..initialize_config()};
		
	let sse_port = comm::Port();
	let sse_chan = comm::Chan(&sse_port);
	let iconfig = config_to_conn(&config, sse_chan);
	
	assert validate_config(&iconfig) == ~"No views for the following routes: farewell, greeting";
}

#[test]
fn views_must_have_routes()
{
	let config = Config {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: path::from_str(~"server/html"),
		routes: ~[(~"GET", ~"/", ~"home")],
		views: ~[(~"home",  missing_view), (~"greeting",  missing_view), (~"goodbye",  missing_view)],
		..initialize_config()};
		
	let sse_port = comm::Port();
	let sse_chan = comm::Chan(&sse_port);
	let iconfig = config_to_conn(&config, sse_chan);
	
	assert validate_config(&iconfig) == ~"No routes for the following views: goodbye, greeting";
}

#[test]
fn root_must_have_required_files()
{
	let config = Config {
		hosts: ~[~"localhost"],
		server_info: ~"unit test",
		resources_root: path::from_str(~"/tmp"),
		routes: ~[(~"GET", ~"/", ~"home")],
		views: ~[(~"home",  missing_view)],
		..initialize_config()};
		
	let sse_port = comm::Port();
	let sse_chan = comm::Chan(&sse_port);
	let iconfig = config_to_conn(&config, sse_chan);
	
	assert validate_config(&iconfig) == ~"Missing required files: forbidden.html, home.html, not-found.html, not-supported.html";
}

