// http://www.w3.org/Protocols/rfc2616/rfc2616.html
use socket::*;
use connection::{handle_connection};

export start;

/// Startup the server.
/// 
/// Currently this will run until a client does a GET on '/shutdown' in which case exit is called.
fn start(+config: configuration::config)
{
	let port = comm::Port::<uint>();
	let chan = comm::Chan::<uint>(port);
	let mut count = vec::len(config.hosts);
	
	// Accept connections from clients on one or more interfaces.
	do task::spawn
	{
		for vec::each(config.hosts)
		|host|
		{
			let h = copy host;
			let config2 = copy(config);
			do task::spawn
			{
				let r = do result::chain(socket::bind_socket(h, config2.port))
				|shandle|
				{
					do result::chain(socket::listen(shandle, 10i32))
						|shandle| {attach(copy(config2), h, shandle)}
				};
				if result::is_err(r)
				{
					error!("Couldn't start web server at %s: %s", h, result::get_err(r));
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

fn attach(+config: configuration::config, host: ~str, shandle: @socket::socket_handle) -> Result<@socket::socket_handle, ~str>
{
	info!("server is listening for new connections on %s:%?", host, config.port);
	do result::chain(socket::accept(shandle))
	|result|
	{
		info!("connected to client at %s", result.remote_addr);
		let config2 = copy config;
		let host2 = copy host;
		let ra2 = copy result.remote_addr;
		let fd2 = copy result.fd;
		do task::spawn {handle_connection(config2, fd2, host2, ra2)};
		result::Ok(shandle)
	};
	attach(config, host, shandle)
}

