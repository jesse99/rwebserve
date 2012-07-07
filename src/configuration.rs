/// Types and functions used to configure rwebserve.

/// Configuration information for the web server.
/// 
/// * hosts are the ip addresses (or "localhost") that the server binds to.
/// * port is the TCP port that the server listens on.
/// * server_info is included in the HTTP response and should include the server name and version.
/// * resources_root should be a path to where the files associated with URLs are loaded from.
/// * routes: maps HTTP methods and URI templates ("/hello/{name}") to route names ("greeting"). 
/// To support non text/html types append the template with "<some/type>".
/// * views: maps route names to view handler functions.
/// * static: used to handle URIs that don't match routes, but are found beneath resources_root.
/// * missing: used to handle URIs that don't match routes, and are not found beneath resources_root.
/// * static_types: maps file extensions (including the period) to mime types.
/// * read_error: html used when a file fails to load. Must include {{request-path}} template.
/// * load_rsrc: maps a path rooted at resources_root to a resource body.
/// * valid_rsrc: returns true if a path rooted at resources_root points to a file.
/// * settings: arbitrary key/value pairs passed into view handlers. If debug is "true" rwebserve debugging 
/// code will be enabled (among other things this will default the Cache-Control header to "no-cache").
/// 
/// initialize_config can be used to initialize some of these fields.
type config = {
	hosts: ~[str],
	port: u16,
	server_info: str,
	resources_root: str,
	routes: ~[(str, str, str)],					// better to use hashmap, but hashmaps cannot be sent
	views: ~[(str, response_handler)],
	static: response_handler,
	missing: response_handler,
	static_types: ~[(str, str)],
	read_error: str,
	load_rsrc: rsrc_loader,
	valid_rsrc: rsrc_exists,
	settings: ~[(str, str)]};
	
/// Information about incoming http requests. Passed into view functions.
/// 
/// * version: HTTP version.
/// * method: "GET", "PUSH", "POST", etc.
/// * local_addr: ip address of the server.
/// * remote_addr: ip address of the client (or proxy).
/// * path: path component of the URL.
/// * matches: contains entries from request_path matching a routes URI template.
/// * headers: headers from the http request. Note that the names are lower cased.
/// * body: body of the http request.
type request = {
	version: str,
	method: str,
	local_addr: str,
	remote_addr: str,
	path: str,
	matches: hashmap<str, str>,
	headers: hashmap<str, str>,
	body: str};

/// Returned by view functions and used to generate http response messages.
/// 
/// * status: the status code and message for the response, defaults to "200 OK".
/// * headers: the HTTP headers to be included in the response.
/// * body: contents of the section after headers.
/// * template: path relative to resources_root containing a template file.
/// * context: hashmap used when rendering the template file.
/// 
/// If template is not empty then body should be empty. If body is not empty then
/// headers["Content-Type"] should usually be explicitly set.
type response = {
	status: str,
	headers: hashmap<str, str>,
	body: str,
	template: str,
	context: hashmap<str, mustache::data>};

/// Function used to generate an HTTP response.
/// 
/// On entry reponse.status will typically be set to \"200 OK\". response.headers will include something like the following:
/// * Server: whizbang server 1.0
/// * Content-Length: 0 (if non-zero rwebserve will not compute the body length)
/// * Content-Type:  text/html; charset=UTF-8
/// Context will be initialized with:
/// * request-path: the path component of the url within the client request message (e.g. '/home').
/// * status-code: the code that will be included in the response message (e.g. '200' or '404').
/// * status-mesg: the code that will be included in the response message (e.g. 'OK' or 'Not Found').
/// * request-version: HTTP version of the request message (e.g. '1.1').
/// 
/// On exit the response will have:
/// * status: is normally left unchanged.
/// * headers: existing headers may be modified and new ones added (e.g. to control caching).
/// * matches: should not be changed.
/// * template: should be set to a path relative to resources_root.
/// * context: new entries will often be added. If template is not actually a template file empty the context.
/// 
/// After the function returns a base-path entry is added to the response.context with the url to the directory containing the template file.
type response_handler = fn~ (hashmap<str, str>, request, response) -> response;

/// Maps a path rooted at resources_root to a resource body.
type rsrc_loader = fn~ (str) -> result::result<str, str>;

/// Returns true if a path rooted at resources_root points to a file.
type rsrc_exists = fn~ (str) -> bool;

/// Initalizes several config fields.
/// 
/// * port is initialized to 80.
/// * static is initialized to a reasonable view handler.
/// * missing is initialized to a view that assume a \"not-found.html\" is at the root.
/// * static_types is given entries for audio, image, video, and text extensions.
/// * read_error is initialized to a reasonable English language html error message.
/// * load_rsrc: is initialized to io::read_whole_file_str.
/// * valid_rsrc: is initialized to os::path_exists.
fn initialize_config() -> config
{
	{
	hosts: ~[""],
	port: 80_u16,
	server_info: "",
	resources_root: "",
	routes: ~[],
	views: ~[],
	static: static_view,
	missing: missing_view,
	static_types: ~[
		(".m4a", "audio/mp4"),
		(".m4b", "audio/mp4"),
		(".mp3", "audio/mpeg"),
		(".wav", "audio/vnd.wave"),
		
		(".gif", "image/gif"),
		(".jpeg", "image/jpeg"),
		(".jpg", "image/jpeg"),
		(".png", "image/png"),
		(".tiff", "image/tiff"),
		
		(".css", "text/css"),
		(".csv", "text/csv"),
		(".html", "text/html"),
		(".htm", "text/html"),
		(".txt", "text/plain"),
		(".text", "text/plain"),
		(".xml", "text/xml"),
		
		(".js", "text/javascript"),
		
		(".mp4", "video/mp4"),
		(".mov", "video/quicktime"),
		(".mpg", "video/mpeg"),
		(".mpeg", "video/mpeg"),
		(".qt", "video/quicktime")],
	read_error: "<!DOCTYPE html>
<meta charset=utf-8>

<title>Error 403 (Forbidden)!</title>

<p>Could not read URL {{request-path}}.</p>",
	load_rsrc: io::read_whole_file_str,
	valid_rsrc: os::path_exists,
	settings: ~[]}
}

type route = {method: str, template: ~[uri_template::component], mime_type: str, route: str};

// Task specific version of config. Should be identical to config (except that it uses
// hashmaps instead of arrays of tuples).
type internal_config = {
	hosts: ~[str],
	port: u16,
	server_info: str,
	resources_root: str,
	route_list: ~[route],
	views_table: hashmap<str, response_handler>,
	static: response_handler,
	missing: response_handler,
	static_type_table: hashmap<str, str>,
	read_error: str,
	load_rsrc: rsrc_loader,
	valid_rsrc: rsrc_exists,
	settings: hashmap<str, str>};

// Default config.static view handler.
fn static_view(_settings: hashmap<str, str>, _request: request, response: response) -> response
{
	let path = mustache::render_str("{{request-path}}", response.context);
	{body: "", template: path, context: std::map::str_hash() with response}
}

// Default config.missing handler. Assumes that there is a "not-found.html"
// file at the resource root.
fn missing_view(_settings: hashmap<str, str>, _request: request, response: response) -> response
{
	{template: "not-found.html" with response}
}

fn to_route(input: (str, str, str)) -> route
{
	alt input
	{
		(method, template_str, route)
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
					(template_str, "text/html")
				};
			
			{method: method, template: uri_template::compile(template), mime_type: mime_type, route: route}
		}
	}
}

fn config_to_internal(config: config) -> internal_config
{
	{	hosts: config.hosts,
		port: config.port,
		server_info: config.server_info,
		resources_root: config.resources_root,
		route_list: vec::map(config.routes, to_route),
		views_table: std::map::hash_from_strs(config.views),
		static: copy(config.static),
		missing: copy(config.missing),
		static_type_table: std::map::hash_from_strs(config.static_types),
		read_error: config.read_error,
		load_rsrc: copy(config.load_rsrc),
		valid_rsrc: copy(config.valid_rsrc),
		settings: std::map::hash_from_strs(config.settings)}
}

fn validate_config(config: internal_config) -> str
{
	let mut errors = ~[];
	
	if vec::is_empty(config.hosts)
	{
		vec::push(errors, "Hosts is empty.");
	}
	
	for vec::each(config.hosts)
	|host|
	{
		if str::is_empty(host)
		{
			vec::push(errors, "Host is empty.");
		}
	};
	
	if config.port < 1024_u16 && config.port != 80_u16
	{
		vec::push(errors, "Port should be 80 or 1024 or above.");
	}
	
	if str::is_empty(config.server_info)
	{
		vec::push(errors, "server_info is empty.");
	}
	
	if str::is_empty(config.resources_root)
	{
		vec::push(errors, "resources_root is empty.");
	}
	else if !os::path_is_dir(config.resources_root)
	{
		vec::push(errors, "resources_root is not a directory.");
	}
	
	let mut names = ~[];
	for vec::each(~["forbidden.html", "home.html", "not-found.html", "not-supported.html"])
	|name|
	{
		let path = path::connect(config.resources_root, name);
		if !os::path_exists(path)
		{
			vec::push(names, name);
		}
	};
	if vec::is_not_empty(names)
	{
		vec::push(errors, "Missing required files: " + str::connect(names, ", "));
	}
	
	if str::is_empty(config.read_error)
	{
		vec::push(errors, "read_error is empty.");
	}
	
	let mut missing_routes = ~[];
	let mut routes = ~[];
	for config.route_list.each()
	|entry|
	{
		let route = entry.route;
		if !config.views_table.contains_key(route)
		{
			vec::push(missing_routes, route);
		}
		vec::push(routes, route);
	};
	if vec::is_not_empty(missing_routes)
	{
		fn le(&&a: str, &&b: str) -> bool {a <= b}
		let missing_routes = std::sort::merge_sort(le, missing_routes);		// order depends on hash, but for unit tests we want to use something more consistent
		
		vec::push(errors, #fmt["No views for the following routes: %s", str::connect(missing_routes, ", ")]);
	}
	
	let mut missing_views = ~[];
	for config.views_table.each_key()
	|route|
	{
		if !vec::contains(routes, route)
		{
			vec::push(missing_views, route);
		}
	};
	if vec::is_not_empty(missing_views)
	{
		fn le(&&a: str, &&b: str) -> bool {a <= b}
		let missing_views = std::sort::merge_sort(le, missing_views);
		
		vec::push(errors, #fmt["No routes for the following views: %s", str::connect(missing_views, ", ")]);
	}
	
	ret str::connect(errors, " ");
}

#[test]
fn routes_must_have_views()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/", "home"), ("GET", "/hello", "greeting"), ("GET", "/goodbye", "farewell")]/~,
		views: [("home",  missing_view)]/~
		with initialize_config()};
	let iconfig = config_to_internal(config);
	
	assert validate_config(iconfig) == "No views for the following routes: farewell, greeting";
}

#[test]
fn views_must_have_routes()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "server/html",
		routes: [("GET", "/", "home")]/~,
		views: [("home",  missing_view), ("greeting",  missing_view), ("goodbye",  missing_view)]/~
		with initialize_config()};
	let iconfig = config_to_internal(config);
	
	assert validate_config(iconfig) == "No routes for the following views: goodbye, greeting";
}

#[test]
fn root_must_have_required_files()
{
	let config = {
		hosts: ["localhost"]/~,
		server_info: "unit test",
		resources_root: "/tmp",
		routes: [("GET", "/", "home")]/~,
		views: [("home",  missing_view)]/~
		with initialize_config()};
	let iconfig = config_to_internal(config);
	
	assert validate_config(iconfig) == "Missing required files: forbidden.html, home.html, not-found.html, not-supported.html";
}
