/// Server-sent event support.
// http://www.w3.org/TR/2009/WD-html5-20090212/comms.html
// http://dev.w3.org/html5/eventsource
use std::map::*;
use path::{Path};
use mustache::*;
//use connection::{conn_config};
//use request::{make_initial_response};

// TODO: should be in connection.rs (see rust bug 3352)
// Like config except that it is connection specific, uses hashmaps, and adds some fields for sse.
type conn_config =
{
	hosts: ~[~str],
	port: u16,
	server_info: ~str,
	resources_root: Path,
	route_list: ~[configuration::route],
	views_table: hashmap<~str, configuration::response_handler>,
	static: configuration::response_handler,
	sse_openers: hashmap<~str, open_sse>,	// key is a GET path
	sse_tasks: hashmap<~str, control_chan>,	// key is a GET path
	sse_push: comm::Chan<~str>,
	missing: configuration::response_handler,
	static_type_table: hashmap<~str, ~str>,
	read_error: ~str,
	load_rsrc: configuration::rsrc_loader,
	valid_rsrc: configuration::rsrc_exists,
	settings: hashmap<~str, ~str>,
};

// TODO: should be in connection.rs (see rust bug 3352)
fn config_to_conn(config: configuration::config, push: comm::Chan<~str>) -> conn_config
{
	{	hosts: config.hosts,
		port: config.port,
		server_info: config.server_info,
		resources_root: config.resources_root,
		route_list: vec::map(config.routes, connection::to_route),
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

// TODO: should be in request.rs (see rust bug 3352)
fn make_initial_response(config: conn_config, status_code: ~str, status_mesg: ~str, mime_type: ~str, request: configuration::request) -> configuration::response
{
	let headers = std::map::hash_from_strs(~[
		(~"Content-Type", mime_type),
		(~"Date", std::time::now_utc().rfc822()),
		(~"Server", config.server_info),
	]);
	
	if config.settings.contains_key(~"debug") && config.settings.get(~"debug") == ~"true"
	{
		headers.insert(~"Cache-Control", ~"no-cache");
	}
	
	let context = std::map::box_str_hash();
	context.insert(@~"request-path", mustache::Str(@request.path));
	context.insert(@~"status-code", mustache::Str(@status_code));
	context.insert(@~"status-mesg", mustache::Str(@status_mesg));
	context.insert(@~"request-version", mustache::Str(@request.version));
	
	{status: status_code + ~" " + status_mesg, headers: headers, body: ~"", template: ~"", context: context}
}

/// Called by the server to spin up a task for an sse session. Returns a
/// channel that the server uses to communicate with the task.
///
/// The hashmap contains the config settings. The push_chan allows the
/// task to push data to the client.
type open_sse = fn~ (hashmap<~str, ~str>, request: configuration::request, push_chan) -> control_chan;

/// The channel used by server tasks to send data to a client.
///
/// In the simplest case the data would contain a single line with the format: 
/// "data: arbitrary text\n". For more details see [event stream](http://dev.w3.org/html5/eventsource/#event-stream-interpretation).
type push_chan = comm::Chan<~str>;

/// The port sse tasks use to respond to events from the server.
type control_port = comm::Port<control_event>;

/// The channel used by the server to communicate with sse tasks.
type control_chan = comm::Chan<control_event>;

/// The data sent by the control_chan to a task.
///
/// refresh_event will be sent according to the reconnection time used by the
/// client (typically 3s if not set using a retry field in the pushed data).
///
/// close_event will be sent if the tcp connection is dropped or the client
/// closes the EventSource.
enum control_event
{
	refresh_event,
	close_event,
}

// This is invoked when the client sends a GET on behalf of an event source.
fn process_sse(config: conn_config, request: configuration::request) -> (configuration::response, ~str)
{
	let mut code = ~"200";
	let mut mesg = ~"OK";
	let mut mime = ~"text/event-stream; charset=utf-8";
	
	match config.sse_tasks.find(request.path)
	{
		option::Some(sse) =>
		{
			comm::send(sse, refresh_event);
		}
		option::None =>
		{
			if !open_sse(config, request, config.sse_push)
			{
				code = ~"404";
				mesg = ~"Not Found";
				mime = ~"text/event-stream";
			}
		}
	}
	
	let response = make_initial_response(config, code, mesg, mime, request);
	response.headers.insert(~"Transfer-Encoding", ~"chunked");
	response.headers.insert(~"Cache-Control", ~"no-cache");
	(response, ~"\n\n")
}

// TODO: Chrome, at least, doesn't seem to close EventSources so we need to time these out.
fn open_sse(config: conn_config, request: configuration::request, push_data: push_chan) -> bool
{
	match config.sse_openers.find(request.path)
	{
		option::Some(opener) =>
		{
			info!("opening sse for %s", request.path);
			let sse = opener(config.settings, request, push_data);
			config.sse_tasks.insert(request.path, sse);
			true
		}
		option::None =>
		{
			error!("%s was not found in sse_openers", request.path);
			false
		}
	}
}

fn close_sses(config: conn_config)
{
	info!("closing all sse");
	for config.sse_tasks.each_value
	|control_ch|
	{
		comm::send(control_ch, close_event);
	};
}

fn make_response(config: conn_config) -> configuration::response
{
	let headers = std::map::hash_from_strs(~[
		(~"Cache-Control", ~"no-cache"),
		(~"Content-Type", ~"text/event-stream; charset=utf-8"),
		(~"Date", std::time::now_utc().rfc822()),
		(~"Server", config.server_info),
		(~"Transfer-Encoding", ~"chunked"),
	]);
	
	{status: ~"200 OK", headers: headers, body: ~"", template: ~"", context: std::map::box_str_hash()}
}

