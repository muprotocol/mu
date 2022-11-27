#!/bin/bash

################################################################################
#
# A script to run the example as an integration test. It starts up a localnet
# and executes the current directory's rust binary.
#
# Usage:
#
# ./run-tests.sh
#
# The anchor cli and solana cli must be installed.
#
# cargo install --git https://github.com/coral-xyz/anchor anchor-cli --locked
#
################################################################################

set -euox pipefail

main() {
    #
    # Build programs.
    #
    local mu_pid="2MZLka8nfoAf1LKCCbgCw5ZXfpMbKGDuLjQ88MNMyti2"
    cd ../marketplace
    anchor build
    cd ../cli/

    #
    # Bootup validator.
    #
	export RUST_LOG=solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=debug,solana_bpf_loader=debug,solana_rbpf=debug
    solana-test-validator -r \
				--log \
				-l target/test-ledger \
				--bpf-program $mu_pid ../marketplace/target/deploy/marketplace.so \
				&> target/test-validator.log &
    validator_pid=$!
    sleep 5

    #
    # Initialize mu
    #
	echo $HOME
	export BROWSER='' ANCHOR_WALLET=$(echo "${HOME}/.config/solana/id.json")
    cd ../marketplace
	anchor run initialize-mu
	cd ../cli

    #
    # Run Test.
    #
    # cargo test
}

cleanup() {
    # kill -9 $validator_pid || true
    kill -9 $$ || true
    wait || true
}

trap_add() {
    trap_add_cmd=$1; shift || fatal "${FUNCNAME} usage error"
    for trap_add_name in "$@"; do
        trap -- "$(
            extract_trap_cmd() { printf '%s\n' "${3:-}"; }
            eval "extract_trap_cmd $(trap -p "${trap_add_name}")"
            printf '%s\n' "${trap_add_cmd}"
        )" "${trap_add_name}" \
            || fatal "unable to add to trap ${trap_add_name}"
    done
}

declare -f -t trap_add
trap_add 'cleanup' EXIT
main
