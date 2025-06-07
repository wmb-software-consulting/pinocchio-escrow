

build:
	cargo build-sbf

test:
	cargo build-sbf
	cargo test --features test --test tests

