# shellcheck shell=sh
# Clean-room build environment for the gate (T764; spec/local-ci "Build caching").
#
# Sourced (not run) by scripts/check and scripts/test-clean-room. POSIX sh.
#
#     . "$(dirname "$0")/clean-room.sh"
#     vix_clean_room            # honours VIX_CLEAN_ROOM (default 1)
#
# Why: a compiler-cache wrapper (kache, sccache) sits between cargo and rustc,
# rustdoc, the C compiler and the test runner. On the maintainer's Mac the
# `kache` wrappers made the gate fail spuriously (bogus linker errors; rustdoc
# "daemon compilation did not complete" / "cannot read compiler process
# identity") under concurrent load. A gate that can pass or fail for reasons
# unrelated to the tree is worse than a slow one, so the gate runs without any
# wrapper unless VIX_CLEAN_ROOM=0.
#
# What it does when VIX_CLEAN_ROOM is not 0:
#   - drops every PATH directory whose path mentions kache or sccache (the
#     `cargo`/`rustdoc`/`cc` shim directories);
#   - empties RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER and CARGO_BUILD_RUSTC_WRAPPER
#     (empty overrides ~/.cargo/config.toml `build.rustc-wrapper`);
#   - points RUSTC / RUSTDOC at the plain tools when they name a wrapper (an
#     env var is the only thing that overrides config `build.rustdoc`; PATH
#     alone does not);
#   - UNSETS (never empties: cargo rejects an empty runner) any
#     CARGO_TARGET_*_RUNNER that names a wrapper, and CC_KNOWN_WRAPPER_CUSTOM;
#   - sets HOST_CC=cc and HOST_CXX=c++ (config `[env]` HOST_CC="kache cc" never
#     overrides a variable that is already set).
# It never touches CARGO_TARGET_DIR (only reports it) and never edits any
# config file: ~/.cargo/config.toml and the shims stay exactly as the user
# left them.

# vix_is_wrapper_text TEXT: succeeds when TEXT names a compiler-cache wrapper.
vix_is_wrapper_text() {
	case "$1" in
	*kache* | *sccache*) return 0 ;;
	*) return 1 ;;
	esac
}

# vix_strip_path PATH: print PATH without wrapper directories.
vix_strip_path() {
	_out=""
	_last=""
	_rest=$1
	while :; do
		case "$_rest" in
		*:*)
			_d=${_rest%%:*}
			_rest=${_rest#*:}
			;;
		*)
			_d=$_rest
			_rest=""
			_last=1
			;;
		esac
		if ! vix_is_wrapper_text "$_d"; then
			_out="${_out:+$_out:}$_d"
		fi
		[ -z "${_last:-}" ] || break
	done
	printf '%s' "$_out"
}

# vix_clean_room: apply (or skip) the clean room, then print one line saying
# which environment the gate runs in, plus the CARGO_TARGET_DIR advice.
vix_clean_room() {
	if [ "${VIX_CLEAN_ROOM:-1}" = 0 ]; then
		echo "==> environment: inherited (VIX_CLEAN_ROOM=0; RUSTC_WRAPPER=${RUSTC_WRAPPER:-none}) -- a wrapper may make the gate flaky"
	else
		_removed=""
		_new=$(vix_strip_path "$PATH")
		[ "$_new" = "$PATH" ] || _removed=" path-stripped"
		PATH=$_new
		RUSTC_WRAPPER=
		RUSTC_WORKSPACE_WRAPPER=
		CARGO_BUILD_RUSTC_WRAPPER=
		export PATH RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER CARGO_BUILD_RUSTC_WRAPPER
		if vix_is_wrapper_text "${RUSTC:-}"; then RUSTC=rustc; export RUSTC; fi
		# Always plain rustdoc: ~/.cargo/config.toml may pin build.rustdoc to a shim.
		if [ -z "${RUSTDOC:-}" ] || vix_is_wrapper_text "$RUSTDOC"; then
			RUSTDOC=rustdoc
			export RUSTDOC
		fi
		for _v in $(env | sed -n 's/^\(CARGO_TARGET_[A-Za-z0-9_]*_RUNNER\)=.*/\1/p'); do
			eval "_val=\${$_v}"
			# shellcheck disable=SC2154 # _val is assigned by the eval above
			if vix_is_wrapper_text "$_val"; then
				unset "$_v"
				_removed="$_removed unset-$_v"
			fi
		done
		if vix_is_wrapper_text "${CC_KNOWN_WRAPPER_CUSTOM:-}"; then unset CC_KNOWN_WRAPPER_CUSTOM; fi
		HOST_CC=cc
		HOST_CXX=c++
		export HOST_CC HOST_CXX
		echo "==> environment: clean room (no compiler-cache wrapper; HOST_CC=cc HOST_CXX=c++;${_removed:- nothing to strip}; VIX_CLEAN_ROOM=0 opts out)"
	fi
	if [ -n "${CARGO_TARGET_DIR:-}" ]; then
		echo "    CARGO_TARGET_DIR=$CARGO_TARGET_DIR (a cargo build lock serializes every gate sharing it: give each concurrent agent or worktree its own)"
	else
		echo "    CARGO_TARGET_DIR unset (using ./target, private to this worktree). Gating several worktrees at once: still one target dir each; never share one"
	fi
}
