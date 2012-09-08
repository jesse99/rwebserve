/// Server-sent event support.
// http://www.w3.org/TR/2009/WD-html5-20090212/comms.html
// http://dev.w3.org/html5/eventsource
use std::map::*;
use path::{Path};
use mustache::*;

/// Called by the server to spin up a task for an sse session. Returns a
/// channel that the server uses to communicate with the task.
///
/// The hashmap contains the config settings. The PushChan allows the
/// task to push data to the client.
type OpenSse = fn~ (hashmap<~str, ~str>, request: configuration::Request, PushChan) -> ControlChan;

/// The channel used by server tasks to send data to a client.
///
/// In the simplest case the data would contain a single line with the format: 
/// "data: arbitrary text\n". For more details see [event stream](http://dev.w3.org/html5/eventsource/#event-stream-interpretation).
type PushChan = comm::Chan<~str>;

/// The port sse tasks use to respond to events from the server.
type ControlPort = comm::Port<ControlEvent>;

/// The channel used by the server to communicate with sse tasks.
type ControlChan = comm::Chan<ControlEvent>;

/// The data sent by the ControlChan to a task.
///
/// RefreshEvent will be sent according to the reconnection time used by the
/// client (typically 3s if not set using a retry field in the pushed data).
///
/// CloseEvent will be sent if the tcp connection is dropped or the client
/// closes the EventSource.
enum ControlEvent
{
	RefreshEvent,
	CloseEvent,
}

// This is invoked when the client sends a GET on behalf of an event source.
fn process_sse(config: connection::ConnConfig, request: configuration::Request) -> (configuration::Response, ~str)
{
	let mut code = ~"200";
	let mut mesg = ~"OK";
	let mut mime = ~"text/event-stream; charset=utf-8";
	
	match config.sse_tasks.find(request.path)
	{
		option::Some(sse) =>
		{
			comm::send(sse, RefreshEvent);
		}
		option::None =>
		{
			if !OpenSse(config, request, config.sse_push)
			{
				code = ~"404";
				mesg = ~"Not Found";
				mime = ~"text/event-stream";
			}
		}
	}
	
	let response = request::make_initial_response(config, code, mesg, mime, request);
	response.headers.insert(~"Transfer-Encoding", ~"chunked");
	response.headers.insert(~"Cache-Control", ~"no-cache");
	(response, ~"\n\n")
}

// TODO: Chrome, at least, doesn't seem to close EventSources so we need to time these out.
fn OpenSse(config: connection::ConnConfig, request: configuration::Request, push_data: PushChan) -> bool
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

fn close_sses(config: connection::ConnConfig)
{
	info!("closing all sse");
	for config.sse_tasks.each_value
	|control_ch|
	{
		comm::send(control_ch, CloseEvent);
	};
}

fn make_response(config: connection::ConnConfig) -> configuration::Response
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

