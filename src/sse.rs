/// Server-sent event support.
// http://www.w3.org/TR/2009/WD-html5-20090212/comms.html
// http://dev.w3.org/html5/eventsource
import connection::{conn_config};
import request::{make_initial_response};

/// Called by the server to spin up a task for an sse session. Returns a
/// channel that the server uses to communicate with the task.
///
/// The hashmap contains the config settings. The push_chan allows the
/// task to push data to the client.
type open_sse = fn~ (hashmap<~str, ~str>, request: request, push_chan) -> control_chan;

/// The channel used by server tasks to send data to a client.
///
/// In the simplest case the data would contain a single line with the format: 
/// "data: arbitrary text\n". For more details see [event stream](http://dev.w3.org/html5/eventsource/#event-stream-interpretation).
type push_chan = comm::chan<~str>;

/// The port sse tasks use to respond to events from the server.
type control_port = comm::port<control_event>;

/// The channel used by the server to communicate with sse tasks.
type control_chan = comm::chan<control_event>;

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
fn process_sse(config: conn_config, request: request) -> (response, ~str)
{
	let mut code = ~"200";
	let mut mesg = ~"OK";
	let mut mime = ~"text/event-stream; charset=utf-8";
	
	alt config.sse_tasks.find(request.path)
	{
		option::some(sse)
		{
			comm::send(sse, refresh_event);
		}
		option::none
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
fn open_sse(config: conn_config, request: request, push_data: push_chan) -> bool
{
	alt config.sse_openers.find(request.path)
	{
		option::some(opener)
		{
			#info["opening sse for %s", request.path];
			let sse = opener(config.settings, request, push_data);
			config.sse_tasks.insert(request.path, sse);
			true
		}
		option::none
		{
			#error["%s was not found in sse_openers", request.path];
			false
		}
	}
}

fn close_sses(config: conn_config)
{
	#info["closing all sse"];
	for config.sse_tasks.each_value
	|control_ch|
	{
		comm::send(control_ch, close_event);
	};
}

fn make_response(config: conn_config) -> response
{
	let headers = std::map::hash_from_strs(~[
		(~"Cache-Control", ~"no-cache"),
		(~"Content-Type", ~"text/event-stream; charset=utf-8"),
		(~"Date", std::time::now_utc().rfc822()),
		(~"Server", config.server_info),
		(~"Transfer-Encoding", ~"chunked"),
	]);
	
	{status: ~"200 OK", headers: headers, body: ~"", template: ~"", context: std::map::str_hash()}
}

