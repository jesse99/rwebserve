/// The module responsible for communication with a particular client.
import socket;
import http_parser::*;
import request::*;
import imap::imap_methods;
import sse;

export handle_client;

// TODO: probably want to use task::unsupervise
fn handle_client(++config: config, fd: libc::c_int, local_addr: str, remote_addr: str)
{
	let sock = @socket::socket_handle(fd);
	let sport = comm::port();
	let sch = comm::chan(sport);
	let eport = comm::port();
	let ech = comm::chan(eport);
	
	let iconfig = config_to_internal(config, ech);
	let err = validate_config(iconfig);
	if str::is_not_empty(err)
	{
		#error["Invalid config: %s", err];
		fail;
	}
	
	do task::spawn {read_requests(fd, sch);}
	loop
	{
		#debug["-----------------------------------------------------------"];
		alt comm::select2(sport, eport)
		{
			either::left(option::some(request))
			{
				let (header, body) = process_request(iconfig, request, local_addr, remote_addr);
				write_response(sock, header, body);
			}
			either::left(option::none)
			{
				sse::close_sses(iconfig);
				break;
			}
			either::right(body)
			{
				let response = sse::make_response(iconfig);
				let (_, body) = make_header_and_body(response, body);
				write_response(sock, "", body);
			}
		}
	}
}

fn read_requests(fd: libc::c_int, poke: comm::chan<option::option<http_request>>)
{
	let sock = @socket::socket_handle(fd);
	let parse = make_parser();
	loop
	{
		let headers = read_headers(sock);
		if str::is_not_empty(headers)
		{
			alt parse(headers)
			{
				result::ok(request)
				{
					if request.headers.contains_key("content-length")
					{
						let body = read_body(sock, request.headers.get("content-length"));
						if str::is_not_empty(body)
						{
							comm::send(poke, option::some({body: body with request}));
						}
						else
						{
							#info["Ignoring %s and %s", headers, body];
						}
					}
					else
					{
						comm::send(poke, option::some(request));
					}
				}
				result::err(mesg)
				{
					#error["Couldn't parse: %s", mesg];
					#error["%s", headers];
				}
			}
		}
		else
		{
			// Client closed connection or there was some sort of error
			// (in which case the client will re-open a connection).
			#info["detached from client"];
			comm::send(poke, option::none);
			break;
		}
	}
}

// TODO: We can't simply do a read for whatever is available because
// clients can issue multple requests. So we need to read the request
// byte by byte until we get a double new-line. If this becomes a bottle
// neck we could do chunked reads, but we'd need to take care to properly
// handle multi-byte utf-8 characters and the split between headers and
// the body.
fn read_headers(sock: @socket::socket_handle) -> str unsafe
{
	let mut buffer = ~[];
	
	while !found_headers(buffer) 
	{
		alt socket::recv(sock, 1u)			// TODO: need a timeout
		{
			result::ok(result)
			{
				vec::push(buffer, result.buffer[0]);
			}
			result::err(mesg)
			{
				#warn["read_headers failed with error: %s", mesg];
				ret "";
			}
		}
	}
	vec::push(buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let mut headers = str::unsafe::from_buf(vec::unsafe::to_ptr(buffer));
		str::unsafe::set_len(headers, vec::len(buffer));		// push adds garbage after the end of the actual elements (i.e. the capacity part)
		#debug["headers: %s", headers];
		headers
	}
	else
	{
		#error["Headers were not utf-8"];	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		""
	}
}

fn found_headers(buffer: [u8]/~) -> bool
{
	if vec::len(buffer) < 4u
	{
		false
	}
	else
	{
		let len = vec::len(buffer);
		buffer[len-4u] == 0x0Du8 && buffer[len-3u] == 0x0Au8 && buffer[len-2u] == 0x0Du8 && buffer[len-1u] == 0x0Au8
	}
}

fn read_body(sock: @socket::socket_handle, content_length: str) -> str unsafe
{
	let total_len = option::get(uint::from_str(content_length));
	
	let mut buffer = ~[];
	vec::reserve(buffer, total_len);
	
	while vec::len(buffer) < total_len 
	{
		alt socket::recv(sock, total_len - vec::len(buffer))			// TODO: need a timeout
		{
			result::ok(result)
			{
				let mut i = 0u;
				while i < result.bytes
				{
					vec::push(buffer, result.buffer[i]);
					i += 1u;
				}
			}
			result::err(mesg)
			{
				#warn["read_body failed with error: %s", mesg];
				ret "";
			}
		}
	}
	vec::push(buffer, 0);		// must be null terminated
	
	if str::is_utf8(buffer)
	{
		let body = str::unsafe::from_buf(vec::unsafe::to_ptr(buffer));
		#debug["body: %s", body];	// note that the log macros truncate long strings 
		body
	}
	else
	{
		#error["Body was not utf-8"];	// TODO: what does the standard say about encodings? do we need to negotiate? or at least return some error response...
		""
	}
}

// TODO: check connection: keep-alive
// TODO: presumbably when we switch to a better socket library we'll be able to handle errors here...
fn write_response(sock: @socket::socket_handle, header: str, body: str)
{
	// It's probably more efficient to do the concatenation rather than two sends because
	// we'll avoid a context switch into the kernel. In any case this seems to increase the
	// likelihood of the network stack putting all of this into a single packet which makes
	// packets easier to analyze.
	let data = header + body;
	do str::as_buf(data) |buffer| {socket::send_buf(sock, buffer, str::len(data))};
}
