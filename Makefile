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
	export RUST_LOG=server=2,rwebserve=2,socket=1 && ./bin/server --admin --root=server/html
	#export RUST_LOG=rwebserve=4,socket=4,::rt::mem=4,::rt::comm=4,::rt::task=4,::rt::dom=4,::rt::cache=4,::rt::upcall=4,::rt::timer=4,::rt::gc=4,::rt::stdlib=4,::rt::kern=4 &&./bin/server --root=server/html

check: bin/test-server
	./bin/test-server

check1: bin/test-server
	export RUST_LOG=rwebserve=2 && ./bin/test-server path_outside_root

# Better to use /usr/local/lib but linking it in with -L /usr/local/lib fails because
# there is a libccore there and in the nested rustc directory.
install:
	install -p `find bin -maxdepth 1 -name "librwebserve*" -type f` /usr/local/lib/rust
	
# You can either use this target (assuming that the libraries are in /usr/local/lib/rust)
# or install them via cargo.
update-libraries:
	cp /usr/local/lib/rust/libmustache-*-0.3pre.* bin
	cp /usr/local/lib/rust/libsocket-*-0.1.* bin
	cp /usr/local/lib/rust/librparse-*-0.6.* bin

# ------------------
# Binary targets 
# We always build the lib because:
# 1) We don't do it that often.
# 2) It's fast.
# 3) The compiler gives it some crazy name like "librwebserve-da45653350eb4f90-0.1.dylib"
# which is dependent on some hash(?) as well as the current platform. (And -o works when
# setting an executable's name, but not libraries).
#
# TODO: enable optimizations (-O), disabled because it can cause problems with back traces
.PHONY : lib
lib:
	$(RUSTC) --out-dir bin -L bin src/rwebserve.rc

bin/test-server: src/rwebserve.rc src/*.rs
	$(RUSTC) -L bin --test -o $@ $<

bin/server: server/src/server.rc server/src/*.rs lib
	$(RUSTC) -L bin -o $@ $<
