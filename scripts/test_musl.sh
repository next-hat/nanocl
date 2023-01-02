export RUSTFLAGS='-C target-feature=-crt-static -L/usr/lib/x86_64-linux-musl -L/lib/x86_64-linux-musl -C linker=musl-gcc -Clink-arg=/usr/lib/x86_64-linux-musl/libc.a -Clpq -Clpgport -Clpgcommon'
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_ALL_STATIC=true
export OPENSSL_STATIC=true
export LIBZ_SYS_STATIC=1
cargo build --release
