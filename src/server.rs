// http://www.w3.org/Protocols/rfc2616/rfc2616.html
import socket;
import connection::*;

export start;

/// Startup the server.
/// 
/// Currently this will run until a client does a GET on '/shutdown' in which case exit is called.
fn start(+config: config)
{
	let port = comm::port::<uint>();
	let chan = comm::chan::<uint>(port);
	let mut count = vec::len(config.hosts);
	
	// Accept connections from clients on one or more interfaces.
	do task::spawn
	{
		for vec::each(config.hosts)
		|host|
		{
			let config2 = copy(config);
			do task::spawn
			{
				let r = do result::chain(socket::bind_socket(host, config2.port))
				|shandle|
				{
					do result::chain(socket::listen(shandle, 10i32))
						|shandle| {attach(copy(config2), host, shandle)}
				};
				if result::is_err(r)
				{
					#error["Couldn't start web server at %s: %s", host, result::get_err(r)];
				}
				comm::send(chan, 1u);
			};
		};
	};
	
	// Exit if we're not accepting on any interfaces (this is an unusual case
	// likely only to happen in the event of errors).
	while count > 0u
	{
		let result = comm::recv(port);
		count -= result;
	}
}

fn attach(+config: config, host: str, shandle: @socket::socket_handle) -> result<@socket::socket_handle, str>
{
	#info["server is listening for new connections on %s:%?", host, config.port];
	do result::chain(socket::accept(shandle))
	|result|
	{
		#info["connected to client at %s", result.remote_addr];
		let config2 = copy(config);
		do task::spawn_sched(task::manual_threads(4)) {handle_connection(config2, result.fd, host, result.remote_addr)};	// TODO: work around for https://github.com/mozilla/rust/issues/2841
		//do task::spawn {handle_connection(config2, result.fd, host, result.remote_addr)};
		result::ok(shandle)
	};
	attach(config, host, shandle)
}

