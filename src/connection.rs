//! The module responsible for communication using a persistent connection to a client.
use socket::*;
use configuration::*;
use http_parser::*;
use imap::*;
use request::{process_request, make_header_and_body};
use sse::*;

// Like config except that it is connection specific, uses hashmaps, and adds some fields for sse.
pub struct ConnConfig
{
	pub hosts: ~[~str],
	pub port: u16,
	pub server_info: ~str,
	pub resources_root: Path,
	pub route_list: ~[configuration::Route],
	pub views_table: HashMap<@~str, configuration::ResponseHandler>,
	pub static_handlers: configuration::ResponseHandler,
	pub sse_openers: HashMap<@~str, OpenSse>,		// key is a GET path
	pub sse_tasks: HashMap<@~str, ControlChan>,	// key is a GET path
	pub sse_push: comm::Chan<~str>,
	pub missing: configuration::ResponseHandler,
	pub static_type_table: HashMap<@~str, @~str>,
	pub read_error: ~str,
	pub load_rsrc: configuration::RsrcLoader,
	pub valid_rsrc: configuration::RsrcExists,
	pub settings: HashMap<@~str, @~str>,
	
	drop {}
}

pub fn config_to_conn(config: &configuration::Config, push: comm::Chan<~str>) -> ConnConfig
{
	ConnConfig {
		hosts: config.hosts,
		port: config.port,
		server_info: config.server_info,
		resources_root: config.resources_root,
		route_list: vec::map(config.routes, to_route),
		views_table: utils::boxed_hash_from_strs(config.views),
		static_handlers: copy(config.static_handlers),
		sse_openers: utils::boxed_hash_from_strs(config.sse),
		sse_tasks: std::map::HashMap(),
		sse_push: push,
		missing: copy(config.missing),
		static_type_table: utils::to_boxed_str_hash(config.static_types),
		read_error: config.read_error,
		load_rsrc: copy(config.load_rsrc),
		valid_rsrc: copy(config.valid_rsrc),
		settings: utils::to_boxed_str_hash(config.settings),
	}
}

// TODO: probably want to use task::unsupervise
pub fn handle_connection(config: Config, fd: libc::c_int, local_addr: ~str, remote_addr: ~str)
{
	let sport = comm::Port();
	let sch = comm::Chan(sport);
	let eport = comm::Port();
	let ech = comm::Chan(eport);
	let sock = @socket::socket_handle(fd);
	
	let iconfig = config_to_conn(&config, ech);
	let err = validate_config(&iconfig);
	if str::is_not_empty(err)
	{
		error!("Invalid config: %s", err);
		fail;
	}
	
	let ra = copy remote_addr;
	do task::spawn_sched(task::SingleThreaded) {read_requests(ra, fd, sch);}
	loop
	{
		debug!("-----------------------------------------------------------");
		match comm::select2(sport, eport)
		{
			either::Left(option::Some(request)) =>
			{
				let (header, body) = process_request(&iconfig, &request, local_addr, remote_addr);
				write_response(sock, header, body);
			}
			either::Left(option::None) =>
			{
				sse::close_sses(&iconfig);
				break;
			}
			either::Right(body) =>
			{
				let response = sse::make_response(&iconfig);
				let (_, body) = make_header_and_body(&response, body);
				write_response(sock, ~"", body);
			}
		}
	}
}

priv fn read_requests(remote_addr: ~str, fd: libc::c_int, poke: comm::Chan<option::Option<HttpRequest>>)
{
	let sock = @socket::socket_handle(fd);		// socket::socket_handle(fd);
	let parse = make_parser();
	loop
	{
		let headers = read_headers(remote_addr, sock);
		if str::is_not_empty(headers)
		{
			match parse(headers)
			{
				result::Ok(request) =>
				{
					if request.headers.contains_key(~"content-length")
					{
						let body = read_body(sock, request.headers.get(~"content-length"));
						if str::is_not_empty(body)
						{
							comm::send(poke, option::Some(HttpRequest {body: body, ..request}));
						}
						else
						{
							info!("Ignoring %s and %s from %s", headers, utils::truncate_str(body, 80), remote_addr);
						}
					}
					else
					{
						comm::send(poke, option::Some(copy request));
					}
				}
				result::Err(mesg) =>
				{
					error!("Couldn't parse: '%s' from %s", mesg, remote_addr);
					error!("%s", headers);
				}
			}
		}
		else
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
priv fn read_headers(remote_addr: ~str, sock: @socket::socket_handle) -> ~str unsafe
{
	let mut buffer = ~[];
	
	while !found_headers(buffer) 
	{
		match socket::recv(sock, 1u)			// TODO: need a timeout
		{
			result::Ok(result) =>
			{
				vec::push(buffer, result.buffer[0]);
			}
			result::Err(mesg) =>
			{
				warn!("read_headers for %s failed with error: %s", remote_addr, mesg);
				return ~"";
			}
		}
	}
	vec::push(buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let mut headers = str::raw::from_buf(vec::raw::to_ptr(buffer));
		str::raw::set_len(headers, vec::len(buffer));		// push adds garbage after the end of the actual elements (i.e. the capacity part)
		debug!("headers: %s", headers);
		headers
	}
	else
	{
		error!("Headers were not utf-8");	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		~""
	}
}

priv fn found_headers(buffer: ~[u8]) -> bool
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

priv fn read_body(sock: @socket::socket_handle, content_length: ~str) -> ~str unsafe
{
	let total_len = option::get(uint::from_str(content_length));
	
	let mut buffer = ~[];
	vec::reserve(buffer, total_len);
	
	while vec::len(buffer) < total_len 
	{
		match socket::recv(sock, total_len - vec::len(buffer))			// TODO: need a timeout
		{
			result::Ok(result) =>
			{
				let mut i = 0u;
				while i < result.bytes
				{
					vec::push(buffer, result.buffer[i]);
					i += 1u;
				}
			}
			result::Err(mesg) =>
			{
				warn!("read_body failed with error: %s", mesg);
				return ~"";
			}
		}
	}
	vec::push(buffer, 0);		// must be null terminated
	
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
priv fn write_response(sock: @socket::socket_handle, header: ~str, body: ~str)
{
	// It's probably more efficient to do the concatenation rather than two sends because
	// we'll avoid a context switch into the kernel. In any case this seems to increase the
	// likelihood of the network stack putting all of this into a single packet which makes
	// packets easier to analyze.
	let data = header + body;
	do str::as_buf(data) |buffer, _len| {socket::send_buf(sock, buffer, str::len(data))};
}

priv fn validate_config(config: &ConnConfig) -> ~str
{
	let mut errors = ~[];
	
	if vec::is_empty(config.hosts)
	{
		vec::push(errors, ~"Hosts is empty.");
	}
	
	for vec::each(config.hosts)
	|host|
	{
		if str::is_empty(*host)
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
	
	if str::is_empty(config.resources_root.to_str())
	{
		vec::push(errors, ~"resources_root is empty.");
	}
	else if !os::path_is_dir(&config.resources_root)
	{
		vec::push(errors, ~"resources_root is not a directory.");
	}
	
	let mut names = ~[];
	for vec::each(~[~"forbidden.html", ~"home.html", ~"not-found.html", ~"not-supported.html"])
	|name|
	{
		let path = config.resources_root.push(*name);
		if !os::path_exists(&path)
		{
			vec::push(names, copy *name);
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
		if !config.views_table.contains_key(@route)
		{
			vec::push(missing_routes, route);
		}
		vec::push(routes, route);
	};
	if vec::is_not_empty(missing_routes)
	{
		pure fn le(a: &~str, b: &~str) -> bool {*a <= *b}
		let missing_routes = std::sort::merge_sort(le, missing_routes);		// order depends on hash, but for unit tests we want to use something more consistent
		
		vec::push(errors, fmt!("No views for the following routes: %s", str::connect(missing_routes, ~", ")));
	}
	
	let mut missing_views = ~[];
	for config.views_table.each_key()
	|route|
	{
		if !vec::contains(routes, *route)
		{
			vec::push(missing_views, *route);
		}
	};
	if vec::is_not_empty(missing_views)
	{
		pure fn le(a: &~str, b: &~str) -> bool {*a <= *b}
		let missing_views = std::sort::merge_sort(le, missing_views);
		
		vec::push(errors, fmt!("No routes for the following views: %s", str::connect(missing_views, ~", ")));
	}
	
	return str::connect(errors, ~" ");
}

pub fn to_route(&&input: (~str, ~str, ~str)) -> Route
{
	match input
	{
		(method, template_str, route) =>
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
			
			Route {method: method, template: uri_template::compile(template), mime_type: mime_type, route: route}
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
		
	let eport = comm::Port();
	let ech = comm::Chan(eport);
	let iconfig = config_to_conn(&config, ech);
	
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
		
	let eport = comm::Port();
	let ech = comm::Chan(eport);
	let iconfig = config_to_conn(&config, ech);
	
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
		
	let eport = comm::Port();
	let ech = comm::Chan(eport);
	let iconfig = config_to_conn(&config, ech);
	
	assert validate_config(&iconfig) == ~"Missing required files: forbidden.html, home.html, not-found.html, not-supported.html";
}

