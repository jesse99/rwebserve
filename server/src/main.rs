import io;
import io::writer_util;
import std::getopts::*;
import std::map::hashmap;
import server = rwebserve::server;

type options = {root: str, admin: bool};

// str constants aren't supported yet.
// TODO: get this (somehow) from the link attribute in the rc file (going the other way
// doesn't work because vers in the link attribute has to be a literal)
fn get_version() -> str
{
	"0.1"
}

fn print_usage()
{
	io::println(#fmt["server %s - sample rrest server", get_version()]);
	io::println("");
	io::println("./server [options] --root=<dir>");
	io::println("--admin      allows web clients to shut the server down");
	io::println("-h, --help   prints this message and exits");
	io::println("--root=DIR   path to the directory containing html files");
	io::println("--version    prints the server version number and exits");
} 

fn parse_command_line(args: [str]) -> options
{
	let opts = [
		optflag("admin"),
		reqopt("root"),
		optflag("h"),
		optflag("help"),
		optflag("version")
	];
	let match = alt getopts(vec::tail(args), opts)
	{
		result::ok(m) {m}
		result::err(f) {io::stderr().write_line(fail_str(f)); libc::exit(1_i32)}
	};
	if opt_present(match, "h") || opt_present(match, "help")
	{
		print_usage();
		libc::exit(0_i32);
	}
	else if opt_present(match, "version")
	{
		io::println(#fmt["server %s", get_version()]);
		libc::exit(0_i32);
	}
	else if vec::is_not_empty(match.free)
	{
		io::stderr().write_line("Positional arguments are not allowed.");
		libc::exit(1_i32);
	}
	{root: opt_str(match, "root"), admin: opt_present(match, "admin")}
}

fn validate_options(options: options)
{
	if !os::path_is_dir(options.root)
	{
		io::stderr().write_line(#fmt["'%s' does not point to a directory.", options.root]);
		libc::exit(1_i32);
	}
}

fn process_command_line(args: [str]) -> str
{
	if vec::len(args) != 2u || !str::starts_with(args[1], "--root=")
	{
		io::stderr().write_line("Expected a --root-path argument pointing to the html pages.");
		libc::exit(1_i32); 
	}
	
	str::slice(args[1], str::len("--root="), str::len(args[1]))
}

fn home_view(_settings: hashmap<str, str>, options: options, _request: server::request, response: server::response) -> server::response
{
	response.context.insert("admin", mustache::bool(options.admin));
	{template: "home.html" with response}
}

fn greeting_view(_settings: hashmap<str, str>, _request: server::request, response: server::response) -> server::response
{
	response.context.insert("user-name", mustache::str("Joe Bob"));
	{template: "hello.html" with response}
}

fn main(args: [str])
{
	#info["starting up sample server"];
	let options = parse_command_line(args);
	validate_options(options);
	
	// This is an example of how additional information can be communicated to
	// a view handler (in this case we're only communicating options.admin so
	// using settings would be simpler).
	let home: server::response_handler = {|settings, request, response| home_view(settings, options, request, response)};	// need the temporary in order to get a unique fn pointer
	
	let config = {
		host: "localhost",
		port: 8088_u16,
		server_info: "sample rrest server " + get_version(),
		resources_root: options.root,
		routes: [("/", "home"), ("/hello", "greeting")],
		views: [("home",  home), ("greeting", greeting_view)],
		settings: [("debug",  "true")]
		with server::initialize_config()};
	
	server::start(config);
	#info["exiting sample server"];		// usually don't land here
}

