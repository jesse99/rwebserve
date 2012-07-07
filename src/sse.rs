/// Server-sent event support.
// http://www.w3.org/TR/2009/WD-html5-20090212/comms.html
// http://dev.w3.org/html5/eventsource

/// Called by the server to spin up a task for an sse session. Returns a
/// channel that the server uses to communicate with the task.
///
/// The hashmap contains the config settings. The push_chan allows the
/// task to push data to the client.
type open_sse = fn~ (hashmap<str, str>, push_chan) -> sse_chan;

/// The channel used by server tasks to send data to a client.
///
/// In the simplest case the data would contain a single line with the format: 
/// "data: arbitrary text\n". For more details see [event stream](http://dev.w3.org/html5/eventsource/#event-stream-interpretation).
type push_chan = comm::chan<str>;		// the data to push

/// The channel used by the server to communicate with sse tasks.
type sse_chan = comm::chan<sse_event>;	// path of the sse session to close down

/// The data sent by the sse_chan to a task.
///
/// refresh_event will be sent according to the reconnection time used by the
/// client (typically 3s if not set using a retry field in the pushed data).
///
/// close_event will be sent if the tcp connection is dropped or the client
/// closes the EventSource.
enum sse_event
{
	refresh_event,
	close_event,
}

