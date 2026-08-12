#!/bin/sh
set -eu

TARGET=${BENCH_TARGET:?BENCH_TARGET is required}
SCRATCH=${BENCH_SCRATCH:?BENCH_SCRATCH is required}
ITERATIONS=${BENCH_ITERATIONS:-5}
IO_MIB=${BENCH_IO_MIB:-256}
FILE_COUNT=${BENCH_FILE_COUNT:-10000}
REPO=${BENCH_REPO:-}
ALT_TARGET=${BENCH_ALT_TARGET:-}

mkdir -p "$SCRATCH"

emit() {
  metric=$1
  value=$2
  unit=$3
  iteration=$4
  printf '%s,%s,%s,%s,%s\n' "$TARGET" "$metric" "$value" "$unit" "$iteration"
}

time_command() {
  timing_file="$SCRATCH/.time.$$"
  if /usr/bin/time -p "$@" >/dev/null 2>"$timing_file"; then
    awk '$1 == "real" { print $2; exit }' "$timing_file"
    rm -f "$timing_file"
    return 0
  fi
  cat "$timing_file" >&2
  rm -f "$timing_file"
  return 1
}

prepare_rust_source() {
  rust_source="$SCRATCH/rust-source"
  rm -rf "$rust_source"
  mkdir -p "$rust_source/src"
  cat >"$rust_source/Cargo.toml" <<'TOML'
[package]
name = "container-runtime-bench"
version = "0.0.0"
edition = "2021"
publish = false
TOML

  : >"$rust_source/src/main.rs"
  i=0
  while [ "$i" -lt 300 ]; do
    printf 'mod m%s;\n' "$i" >>"$rust_source/src/main.rs"
    cat >"$rust_source/src/m$i.rs" <<EOF_MODULE
#[inline(never)]
pub fn mix(mut value: u64) -> u64 {
    let mut i = 0u64;
    while i < 128 {
        value = value.rotate_left(7).wrapping_mul(0x9e3779b185ebca87).wrapping_add(i ^ $i);
        i += 1;
    }
    value
}
EOF_MODULE
    i=$((i + 1))
  done
  printf 'fn main() {\n    let mut value = 1u64;\n' >>"$rust_source/src/main.rs"
  i=0
  while [ "$i" -lt 300 ]; do
    printf '    value ^= m%s::mix(value);\n' "$i" >>"$rust_source/src/main.rs"
    i=$((i + 1))
  done
  printf '    println!("{value}");\n}\n' >>"$rust_source/src/main.rs"
}

run_rust_metrics() {
  command -v cargo >/dev/null 2>&1 || return 0
  prepare_rust_source
  rust_source="$SCRATCH/rust-source"

  i=1
  while [ "$i" -le "$ITERATIONS" ]; do
    cold_target="$SCRATCH/rust-target-cold-$i"
    rm -rf "$cold_target"
    seconds=$(time_command env CARGO_TARGET_DIR="$cold_target" cargo check --offline --manifest-path "$rust_source/Cargo.toml")
    emit rust_cold_check_seconds "$seconds" seconds "$i"

    warm_target="$SCRATCH/rust-target-warm-$i"
    rm -rf "$warm_target"
    CARGO_TARGET_DIR="$warm_target" cargo check --offline --manifest-path "$rust_source/Cargo.toml" >/dev/null 2>&1
    seconds=$(time_command env CARGO_TARGET_DIR="$warm_target" cargo check --offline --manifest-path "$rust_source/Cargo.toml")
    emit rust_warm_check_seconds "$seconds" seconds "$i"

    release_target="$SCRATCH/rust-target-release-$i"
    rm -rf "$release_target"
    seconds=$(time_command env CARGO_TARGET_DIR="$release_target" cargo build --release --offline --manifest-path "$rust_source/Cargo.toml")
    emit rust_release_build_seconds "$seconds" seconds "$i"

    if [ -n "$ALT_TARGET" ]; then
      mkdir -p "$ALT_TARGET"
      alt_check="$ALT_TARGET/check-$i"
      rm -rf "$alt_check"
      seconds=$(time_command env CARGO_TARGET_DIR="$alt_check" cargo check --offline --manifest-path "$rust_source/Cargo.toml")
      emit rust_cold_check_local_target_seconds "$seconds" seconds "$i"

      alt_release="$ALT_TARGET/release-$i"
      rm -rf "$alt_release"
      seconds=$(time_command env CARGO_TARGET_DIR="$alt_release" cargo build --release --offline --manifest-path "$rust_source/Cargo.toml")
      emit rust_release_build_local_target_seconds "$seconds" seconds "$i"
    fi

    i=$((i + 1))
  done
}

run_fio_metrics() {
  command -v fio >/dev/null 2>&1 || return 0
  command -v jq >/dev/null 2>&1 || return 0

  i=1
  while [ "$i" -le "$ITERATIONS" ]; do
    fio_file="$SCRATCH/fio-$i.dat"
    if result=$(fio --name=seqwrite --filename="$fio_file" --size="${IO_MIB}M" --rw=write --bs=1M --direct=1 --ioengine=sync --output-format=json 2>/dev/null); then
      value=$(printf '%s' "$result" | jq -r '.jobs[0].write.bw_bytes / 1048576')
      emit fio_seq_write_mib_s "$value" MiB/s "$i"

      if result=$(fio --name=seqread --filename="$fio_file" --size="${IO_MIB}M" --rw=read --bs=1M --direct=1 --ioengine=sync --output-format=json 2>/dev/null); then
        value=$(printf '%s' "$result" | jq -r '.jobs[0].read.bw_bytes / 1048576')
        emit fio_seq_read_mib_s "$value" MiB/s "$i"
      fi

      if result=$(fio --name=randrw --filename="$fio_file" --size="${IO_MIB}M" --rw=randrw --rwmixread=50 --bs=4k --direct=1 --ioengine=sync --iodepth=1 --runtime=5 --time_based=1 --output-format=json 2>/dev/null); then
        value=$(printf '%s' "$result" | jq -r '.jobs[0].read.iops')
        emit fio_randread_iops "$value" IOPS "$i"
        value=$(printf '%s' "$result" | jq -r '.jobs[0].write.iops')
        emit fio_randwrite_iops "$value" IOPS "$i"
      fi
    else
      printf 'fio direct I/O is unavailable for %s; skipping fio metrics\n' "$TARGET" >&2
    fi
    rm -f "$fio_file"
    i=$((i + 1))
  done
}

run_portable_io_metrics() {
  i=1
  while [ "$i" -le "$ITERATIONS" ]; do
    io_file="$SCRATCH/io-$i.dat"
    rm -f "$io_file"
    seconds=$(time_command sh -c 'dd if=/dev/zero of="$1" bs=1048576 count="$2" >/dev/null 2>&1 && sync' sh "$io_file" "$IO_MIB")
    emit seq_write_seconds "$seconds" seconds "$i"

    seconds=$(time_command dd if="$io_file" of=/dev/null bs=1048576)
    emit seq_read_seconds "$seconds" seconds "$i"

    if command -v openssl >/dev/null 2>&1; then
      seconds=$(time_command openssl dgst -sha256 "$io_file")
      emit sha256_seconds "$seconds" seconds "$i"
    fi

    meta_dir="$SCRATCH/files-$i"
    rm -rf "$meta_dir"
    mkdir -p "$meta_dir"
    seconds=$(time_command env DIR="$meta_dir" COUNT="$FILE_COUNT" sh -c 'n=0; while [ "$n" -lt "$COUNT" ]; do printf x >"$DIR/$n"; n=$((n + 1)); done')
    emit metadata_create_seconds "$seconds" seconds "$i"

    seconds=$(time_command env DIR="$meta_dir" sh -c 'find "$DIR" -type f -print >/dev/null')
    emit metadata_walk_seconds "$seconds" seconds "$i"

    seconds=$(time_command rm -rf "$meta_dir")
    emit metadata_delete_seconds "$seconds" seconds "$i"
    rm -f "$io_file"
    i=$((i + 1))
  done
}

run_repo_metrics() {
  [ -n "$REPO" ] || return 0
  [ -e "$REPO/.git" ] || return 0

  i=1
  while [ "$i" -le "$ITERATIONS" ]; do
    seconds=$(time_command git -C "$REPO" status --porcelain=v1 --untracked-files=all)
    emit git_status_seconds "$seconds" seconds "$i"
    i=$((i + 1))
  done
}

run_portable_io_metrics
run_fio_metrics
run_rust_metrics
run_repo_metrics
