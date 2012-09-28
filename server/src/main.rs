use io::WriterUtil;
use path::{Path};
use mustache::*;
use std::getopts::*;
use std::map::HashMap;
use server = rwebserve::rwebserve;
use server::ImmutableMap;
use ConnConfig = rwebserve::connection::ConnConfig;
use Request = rwebserve::rwebserve::Request;
use Response = rwebserve::rwebserve::Response;
use ResponseHandler = rwebserve::rwebserve::ResponseHandler;

type Options = {root: Path, admin: bool};

// str constants aren't supported yet.
// TODO: get this (somehow) from the link attribute in the rc file (going the other way
// doesn't work because vers in the link attribute has to be a literal)
fn get_version() -> ~str
{
	~"0.1"
}

fn print_usage()
{
	io::println(fmt!("server %s - sample rwebserve server", get_version()));
	io::println(~"");
	io::println(~"./server [options] --root=<dir>");
	io::println(~"--admin      allows web clients to shut the server down");
	io::println(~"-h, --help   prints this message and exits");
	io::println(~"--root=DIR   path to the directory containing html files");
	io::println(~"--version    prints the server version number and exits");
} 

fn parse_command_line(args: &[~str]) -> Options
{
	let opts = ~[
		optflag(~"admin"),
		reqopt(~"root"),
		optflag(~"h"),
		optflag(~"help"),
		optflag(~"version")
	];
	
	let mut t = ~[];
	for vec::eachi(args)		// TODO: tail should work eventually (see https://github.com/mozilla/rust/issues/2770)
	|i, a|
	{
		if i > 0
		{
			vec::push(t, copy(a));
		}
	}
	//let t = vec::tail(args);
	
	let matched = match getopts(t, opts)
	{
		result::Ok(m)	=> copy(m),
		result::Err(f)	=> {io::stderr().write_line(fail_str(f)); libc::exit(1_i32)}
	};
	if opt_present(matched, ~"h") || opt_present(matched, ~"help")
	{
		print_usage();
		libc::exit(0_i32);
	}
	else if opt_present(matched, ~"version")
	{
		io::println(fmt!("server %s", get_version()));
		libc::exit(0_i32);
	}
	else if vec::is_not_empty(matched.free)
	{
		io::stderr().write_line("Positional arguments are not allowed.");
		libc::exit(1_i32);
	}
	{root: path::from_str(opt_str(matched, ~"root")), admin: opt_present(matched, ~"admin")}
}

fn validate_options(options: Options)
{
	if !os::path_is_dir(&options.root)
	{
		io::stderr().write_line(fmt!("'%s' does not point to a directory.", options.root.to_str()));
		libc::exit(1_i32);
	}
}

fn process_command_line(args: ~[~str]) -> ~str
{
	if vec::len(args) != 2u || !str::starts_with(args[1], "--root=")
	{
		io::stderr().write_line("Expected a --root-path argument pointing to the html pages.");
		libc::exit(1_i32); 
	}
	
	str::slice(args[1], str::len("--root="), str::len(args[1]))
}

fn home_view(_config: &ConnConfig, options: &Options, _request: &Request, response: &Response) -> Response
{
	response.context.insert(@~"admin", mustache::Bool(options.admin));
	Response {template: ~"home.html", ..*response}
}

fn greeting_view(_config: &ConnConfig, request: &Request, response: &Response) -> Response
{
	response.context.insert(@~"user-name", mustache::Str(request.matches.get(@~"name")));
	Response {template: ~"hello.html", ..*response}
}

enum StateMesg
{
	AddListener(~str, comm::Chan<int>),	// str is used to identify the listener
	RemoveListener(~str),
	Shutdown,
}

type StateChan = comm::Chan<StateMesg>;

// Like spawn_listener except that it supports custom modes. This allows code that blocks
// within a foreign function to avoid blocking other tasks which may be on its thread.
fn spawn_moded_listener<A: Send>(mode: task::SchedMode, +f: fn~(comm::Port<A>)) -> comm::Chan<A>
{
	let setup_po = comm::Port();
	let setup_ch = comm::Chan(setup_po);
	do task::spawn_sched(mode)
	{
		let po = comm::Port();
		let ch = comm::Chan(po);
		comm::send(setup_ch, ch);
		f(po);
	}
	comm::recv(setup_po)
}

// This is a single task that manages the state for our sample server. Normally this will
// do something like get notified of database changes and send messages to connection
// specific listeners. The listeners could then use server-sent events (sse) to push new
// data to the client.
//
// In this case our state is just an int and we notify listeners when we change it.
fn manage_state() -> StateChan
{
	do spawn_moded_listener(task::ManualThreads(1))
	|state_port: comm::Port<StateMesg>|
	{
		let mut time = 0;
		let listeners = std::map::HashMap();
		loop
		{
			time += 1;
			libc::funcs::posix88::unistd::sleep(1);
			for listeners.each_value |ch| {comm::send(ch, copy(time))};
			
			if state_port.peek()
			{
				match state_port.recv()
				{
					AddListener(key, ch) =>
					{
						let added = listeners.insert(key, ch);
						assert added;
					}
					RemoveListener(key) =>
					{
						listeners.remove(key);
					}
					Shutdown =>
					{
						break;
					}
				}
			}
		}
	}
}

// Each client connection that hits /uptime will cause an instance of this task to run. When
// manage_state tells us that the world has changed we push the new world (an int in
// this case) out to the client.
fn uptime_sse(registrar: StateChan, request: &Request, push: server::PushChan) -> server::ControlChan
{
	let seconds = *request.params.get(@~"units") == ~"s";
	
	do task::spawn_listener
	|control_port: server::ControlPort|
	{
		info!("starting uptime sse stream");
		let notify_port = comm::Port();
		let notify_chan = comm::Chan(notify_port);
		
		let key = fmt!("uptime %?", ptr::addr_of(notify_port));
		comm::send(registrar, AddListener(key, notify_chan));
		
		loop
		{
			let mut time = 0;
			match comm::select2(notify_port, control_port)
			{
				either::Left(new_time) =>
				{
					// To help test the request code we can push uptimes as
					// seconds or minutes based on a query string.
					if seconds
					{
						time = new_time;
					}
					else
					{
						time = new_time/60;
					}
					comm::send(push, fmt!("retry: 5000\ndata: %?\n\n", time));
				}
				either::Right(server::RefreshEvent) =>
				{
					comm::send(push, fmt!("retry: 5000\ndata: %?\n\n", time));
				}
				either::Right(server::CloseEvent) =>
				{
					info!("shutting down uptime sse stream");
					comm::send(registrar, RemoveListener(key));
					break;
				}
			}
		}
	}
}

fn main(args: ~[~str])
{
	let options = parse_command_line(args);
	validate_options(options);
	
	let registrar = manage_state();
	
	// This is an example of how additional information can be communicated to
	// a view handler (in this case we're only communicating options.admin so
	// using settings would be simpler).
	let up: server::OpenSse = |_config: &ConnConfig, request: &Request, push| {uptime_sse(registrar, request, push)};
	
	// TODO: Shouldn't need all of these damned explicit types but rustc currently
	// has problems with type inference woth closures and borrowed pointers.
	let greeting_v: ResponseHandler = greeting_view;
	let home_v: ResponseHandler = |config: &ConnConfig, request: &Request, response: &Response, copy options| {home_view(config, &options, request, response)};
	let shutdown_v: ResponseHandler = |_config: &ConnConfig, _request: &Request, _response: &Response| {info!("received shutdown request"); libc::exit(0)};
	
	let config = server::Config
	{
		hosts: ~[~"localhost", ~"10.6.210.132"],
		port: 8088_u16,
		server_info: ~"sample rrest server " + get_version(),
		resources_root: options.root,
		routes: ~[
			(~"GET", ~"/", ~"home"),
			(~"GET", ~"/shutdown", ~"shutdown"),		// TODO: enable this via debug cfg (or maybe via a command line option)
			(~"GET", ~"/hello/{name}", ~"greeting"),
		],
		views: ~[
			(~"greeting", greeting_v),
			(~"home",  home_v),
			(~"shutdown",  shutdown_v),
		],
		sse: ~[(~"/uptime", up)],
		settings: ~[(~"debug",  ~"true")],
		..server::initialize_config()
	};
	
	server::start(&config);
	info!("exiting sample server");		// usually don't land here
}

