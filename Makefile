RUSTC ?= rustc

# ------------------
# Internal variables
dummy1 := $(shell mkdir bin 2> /dev/null)

# ------------------
# Primary targets
all: lib server

server: lib bin/server

run: lib bin/server
	git web--browse 'http://localhost:8088'
	export RUST_LOG=rwebserve=1,socket=1 && ./bin/server --admin --root=server/html
	#export RUST_LOG=rwebserve=4,socket=4,::rt::mem=4,::rt::comm=4,::rt::task=4,::rt::dom=4,::rt::cache=4,::rt::upcall=4,::rt::timer=4,::rt::gc=4,::rt::stdlib=4,::rt::kern=4 &&./bin/server --root=server/html

check: bin/test-server
	export RUST_LOG=rwebserve=1,rparse=1 && ./bin/test-server

check1: bin/test-server
	export RUST_LOG=rwebserve=2 && ./bin/test-server non_html_route

# Better to use /usr/local/lib but linking it in with -L /usr/local/lib fails because
# there is a libccore there and in the nested rustc directory.
install:
	install -p `find bin -name "librwebserve*" -type f -maxdepth 1` /usr/local/lib/rust
	
# You can either use this target (assuming that the libraries are in /usr/local/lib/rust)
# or install them via cargo.
update-libraries:
	cp /usr/local/lib/rust/libmustache-*-0.1.dylib bin
	cp /usr/local/lib/rust/libsocket-*-0.1.dylib bin
	cp /usr/local/lib/rust/librparse-*-0.3.dylib bin

# ------------------
# Binary targets 
# We always build the lib because:
# 1) We don't do it that often.
# 2) It's fast.
# 3) The compiler gives it some crazy name like "librwebserve-da45653350eb4f90-0.1.dylib"
# which is dependent on some hash(?) as well as the current platform. (And -o works when
# setting an executable's name, but not libraries).
.PHONY : lib
lib:
	$(RUSTC) --out-dir bin -L bin -O src/rwebserve.rc

bin/test-server: src/rwebserve.rc src/*.rs
	$(RUSTC) -g -L bin --test -o $@ $<

bin/server: server/src/server.rc server/src/*.rs bin/librwebserve*
	$(RUSTC) -g -L bin -o $@ $<
