//! Types and functions used to configure rwebserve.
//use mustache::*;
use core::path::{GenericPath};
use std::base64::*;

/// Configuration information for the web server.
/// 
/// * hosts are the ip addresses (or "localhost") that the server binds to.
/// * port is the TCP port that the server listens on.
/// * server_info is included in the HTTP response and should include the server name and version.
/// * resources_root should be a path to where the files associated with URLs are loaded from.
/// * routes: maps HTTP methods ("GET") and URI templates ("hello/{name}") to route names ("greeting"). 
///    To support non-text/html types append the template with "<some/type>".
/// * views: maps route names to view handler functions.
/// * static_handler: used to handle URIs that don't match routes, but are found beneath resources_root.
/// * is_template: returns true if the path is to a mustache template.
/// * sse: maps EventSource path to a function that creates a task to push server-sent events.
/// * missing: used to handle URIs that don't match routes, and are not found beneath resources_root.
/// * static_types: maps file extensions (including the period) to mime types.
/// * read_error: html used when a file fails to load. Must include {{request-path}} template.
/// * load_rsrc: maps a path rooted at resources_root to a resource body.
/// * valid_rsrc: returns true if a path rooted at resources_root points to a file.
/// * settings: arbitrary key/value pairs passed into view handlers. If debug is "true" rwebserve debugging 
/// code will be enabled (among other things this will default the Cache-Control header to "no-cache").
/// 
/// initialize_config can be used to initialize some of these fields. Note that this is sendable and copyable type.
pub struct Config
{
	pub hosts: ~[~str],
	pub port: u16,
	pub server_info: ~str,
	pub resources_root: Path,
	pub routes: ~[(~str, ~str, ~str)],					// better to use hashmap, but hashmaps cannot be sent
	pub views: ~[(~str, ResponseHandler)],
	pub static_handler: ResponseHandler,
	pub is_template: IsTemplateFile,
	pub sse: ~[(~str, OpenSse)],
	pub missing: ResponseHandler,
	pub static_types: ~[(~str, ~str)],
	pub read_error: ~str,
	pub load_rsrc: RsrcLoader,
	pub valid_rsrc: RsrcExists,
	pub settings: ~[(~str, ~str)],
}

/// Information about incoming http requests. Passed into view functions.
/// 
/// * version: HTTP version.
/// * method: "GET", "PUSH", "POST", etc.
/// * local_addr: ip address of the server.
/// * remote_addr: ip address of the client (or proxy).
/// * path: path component of the URL. Note that this does not include the query string.
/// * matches: contains entries from request_path matching a routes URI template.
/// * params: contains entries from the query portion of the URL. Note that the keys may be duplicated.
/// * headers: headers from the http request. Note that the names are lower cased.
/// * body: body of the http request.
pub struct Request
{
	pub version: ~str,
	pub method: ~str,
	pub local_addr: ~str,
	pub remote_addr: ~str,
	pub path: ~str,
	pub matches: HashMap<@~str, @~str>,
	pub params: IMap<@~str, @~str>,
	pub headers: HashMap<@~str, @~str>,
	pub body: ~str,
	
	drop {}			// TODO: enable this (was getting a compiler assert earlier)
}

/// Returned by view functions and used to generate http response messages.
/// 
/// * status: the status code and message for the response, defaults to "200 OK".
/// * headers: the HTTP headers to be included in the response.
/// * body: contents the section after headers.
/// * template: path relative to resources_root containing a template file.
/// * context: hashmap used when rendering the template file.
/// 
/// If template is not empty then body should be empty. If body is not empty then
/// headers["Content-Type"] should usually be explicitly set.
pub struct Response
{
	pub status: ~str,
	pub headers: HashMap<@~str, @~str>,
	pub body: Body,
	pub template: ~str,				// an URL path is very similar to a path::PosixPath, but that is conditionally compiled in
	pub context: HashMap<@~str, mustache::Data>,
	
	drop {}			// TODO: enable this (was getting a compiler assert earlier)
}

/// The part of an HTTP response that comes after the headers.
///
/// The type of an HTTP body is determined by the content-type header. If it is a text mime type
/// then the body with be some flavor of text. However for types like image/png the body will
/// be binary data. This type allows us to avoid copying a text reply to a byte buffer.
pub enum Body
{
	StringBody(@~str),
	BinaryBody(@~[u8]),
	CompoundBody(@[@Body]),		// concatenation of strings and vectors blows if they are large
}

pub impl Body : ToStr
{
	pure fn to_str() -> ~str
	{
		match self
		{
			StringBody(text) =>
			{
				copy *text
			}
			BinaryBody(_binary) =>
			{
				// Not that useful to print binary data and it can be huge so we'll punt on it.
				~"<binary data>"
			}
			CompoundBody(parts) =>
			{
				do parts.foldl(~"") |result, part| {result + part.to_str()}
			}
		}
	}
}

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
pub type ResponseHandler = fn~ (config: &connection::ConnConfig, request: &Request, response: Response) -> Response;

/// Returns true if the file at path should be treated as a mustache template.
pub type IsTemplateFile = fn~ (config: &connection::ConnConfig, path: &str) -> bool;

/// Maps a path rooted at resources_root to a resource body.
pub type RsrcLoader = fn~ (path: &Path) -> result::Result<~[u8], ~str>;

/// Returns true if a path rooted at resources_root points to a file.
pub type RsrcExists = fn~ (path: &Path) -> bool;

pub struct Route
{
	pub method: ~str,
	pub template: ~[uri_template::Component],
	pub mime_type: ~str,
	pub route: ~str,
}

/// Initalizes several config fields.
/// 
/// * port is initialized to 80.
/// * static_handler is initialized to a reasonable view handler.
/// * is_template: is initialized to a function that returns true if the file has an extension of text/plain mime type.
/// * missing is initialized to a view that assume a \"not-found.html\" is at the root.
/// * static_types is given entries for audio, image, video, and text extensions.
/// * read_error is initialized to a reasonable English language html error message.
/// * load_rsrc: is initialized to io::read_whole_file_str.
/// * valid_rsrc: is initialized to os::path_exists && !os::path_is_dir.
pub fn initialize_config() -> Config
{
	Config 
	{
		hosts: ~[~""],
		port: 80_u16,
		server_info: ~"",
		resources_root: GenericPath::from_str(~""),
		routes: ~[],
		views: ~[],
		static_handler: static_view,
		is_template: is_text_file,
		sse: ~[],
		missing: missing_view,
		static_types: ~[
			(~".m4a", ~"audio/mp4"),
			(~".m4b", ~"audio/mp4"),
			(~".mp3", ~"audio/mpeg"),
			(~".wav", ~"audio/vnd.wave"),
			
			(~".gif", ~"image/gif"),
			(~".jpeg", ~"image/jpeg"),
			(~".jpg", ~"image/jpeg"),
			(~".png", ~"image/png"),
			(~".tiff", ~"image/tiff"),
			
			(~".css", ~"text/css"),
			(~".csv", ~"text/csv"),
			(~".html", ~"text/html"),
			(~".htm", ~"text/html"),
			(~".txt", ~"text/plain"),
			(~".text", ~"text/plain"),
			(~".xml", ~"text/xml"),
			
			(~".js", ~"text/javascript"),
			
			(~".mp4", ~"video/mp4"),
			(~".mov", ~"video/quicktime"),
			(~".mpg", ~"video/mpeg"),
			(~".mpeg", ~"video/mpeg"),
			(~".qt", ~"video/quicktime")],
		read_error: ~"<!DOCTYPE html>
	<meta charset=utf-8>
	
	<title>Error 403 (Forbidden)!</title>
	
	<p>Could not read URL {{request-path}}.</p>",
		load_rsrc: io::read_whole_file,
		valid_rsrc: is_valid_rsrc,
		settings: ~[],
	}
}

pub fn is_valid_rsrc(path: &Path) -> bool
{
	os::path_exists(path) && !os::path_is_dir(path)
}

// Default config.missing handler. Assumes that there is a "not-found.html"
// file at the resource root.
pub fn missing_view(_config: &connection::ConnConfig, _request: &Request, response: Response) -> Response
{
	Response {template: ~"not-found.html", ..response}
}

// Default config.static view handler.
//
// Note that this treats files which have a text mime type as mustache templates. More typically
// only files ending with ".mustache" are treated as text. We don't do that because:
// 1) It's expected that expanding a non-template file is not going to be a performance problem.
// 2) Using files like *.html.mustache screws up syntax highlighting in editors.
// 3) Users can install a new is_template closure to do something different.
pub fn static_view(config: &connection::ConnConfig, _request: &Request, response: Response) -> Response
{
	let path = mustache::compile_str("{{request-path}}").render_data(mustache::Map(response.context));
	//let path = mustache::render_str("{{request-path}}", response.context);
	if (config.is_template)(config, path)
	{
		Response {body: StringBody(@~""), template: path, context: std::map::HashMap(), ..response}
	}
	else
	{
		let path = utils::url_to_path(&config.resources_root, path);
		let contents = (config.load_rsrc)(&path);
		if contents.is_ok()
		{
			Response {body: BinaryBody(@result::unwrap(contents)), template: ~"", context: std::map::HashMap(), ..response}
		}
		else
		{
			error!("failed to open %s: %s", path.to_str(), contents.get_err());
			Response {template: ~"not-found.html", ..response}
		}
	}
}

pub fn is_text_file(config: &connection::ConnConfig, path: &str) -> bool
{
	match str::rfind_char(path, '.')
	{
		option::Some(index) =>
		{
			let ext = path.slice(index, path.len());
			match config.static_type_table.find(@ext)
			{
				option::Some(mine_type) => mine_type.starts_with(~"text/"),
				option::None => false,
			}
		}
		option::None =>
		{
			false
		}
	} 
}
