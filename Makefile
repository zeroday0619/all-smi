local:
	cargo run --bin all-smi -- view 

remote:
	cargo run --bin all-smi -- view --hostfile ./hosts.csv

mock:
	cargo run --features mock --bin all-smi-mock-server -- --port-range 10001-10050

release:
	cargo build --release

test:
	cargo test --all
