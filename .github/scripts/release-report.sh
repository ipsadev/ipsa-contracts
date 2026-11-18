#!/usr/bin/env bash
#
# Build the release hash report from the current and previous SHA256SUMS.
#
# A soroban wasm hash is the sha256 of the wasm file, which is the line already
# in SHA256SUMS, so this compares those lines; it uploads nothing. For every
# contract it reports the current hash, the previous release's hash, and whether
# the bytes changed, plus what an operator does about a change. With no previous
# release every contract counts as changed.
#
# Usage:
#   release-report.sh \
#     --current dist/SHA256SUMS \
#     --previous prev/SHA256SUMS|/dev/null \
#     --tag v0.1.0 \
#     --network testnet \
#     --mode incremental|full \
#     [--router C...] \
#     [--addresses deployed.tsv] \
#     --md out.md \
#     --json out.json
#
# deployed.tsv is "<wasm filename><TAB><contract address>[<TAB><client id>]" per
# line, for the light clients this release actually deployed; the client id is
# the corridor the release migrated onto the new client. It is empty on a dry run.

set -euo pipefail

current=""
previous=""
tag=""
network="testnet"
mode="incremental"
router=""
addresses=""
md_out=""
json_out=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --current) current="$2"; shift 2 ;;
    --previous) previous="$2"; shift 2 ;;
    --tag) tag="$2"; shift 2 ;;
    --network) network="$2"; shift 2 ;;
    --mode) mode="$2"; shift 2 ;;
    --router) router="$2"; shift 2 ;;
    --addresses) addresses="$2"; shift 2 ;;
    --md) md_out="$2"; shift 2 ;;
    --json) json_out="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$current" ] || [ ! -f "$current" ]; then
  echo "release-report: --current SHA256SUMS is required" >&2
  exit 2
fi

# name | wasm | category | migrate flag
# category drives the "what to do when changed" guidance:
#   router / app  -> upgrade the live contract in place
#   light-client  -> a new deployment the operator migrates clients onto
#   code          -> a wasm hash an app references, set on that app
#   inactive      -> built but not deployed by a release
contracts=(
  "ibc-router|stellar_ibc_router.wasm|router|"
  "ibc-transfer|stellar_ibc_transfer.wasm|app|"
  "ibc-gmp|stellar_ibc_gmp.wasm|app|"
  "ibc-fast-fill|stellar_ibc_fast_fill.wasm|app|"
  "tendermint light client|stellar_tendermint_light_client.wasm|light-client|"
  "sp1-ethereum light client|stellar_sp1_ethereum_light_client.wasm|light-client|--sp1"
  "ibc-voucher-token|stellar_ibc_voucher_token.wasm|code|"
  "gmp-account|ipsa_gmp_account.wasm|code|"
)

hash_of() {
  local sums="$1" wasm="$2"
  [ -f "$sums" ] || return 0
  awk -v w="$wasm" '$2 == w {print $1; exit}' "$sums"
}

address_of() {
  local wasm="$1"
  [ -n "$addresses" ] && [ -f "$addresses" ] || return 0
  awk -F'\t' -v w="$wasm" '$1 == w {print $2; exit}' "$addresses"
}

migrated_of() {
  local wasm="$1"
  [ -n "$addresses" ] && [ -f "$addresses" ] || return 0
  awk -F'\t' -v w="$wasm" '$1 == w {print $3; exit}' "$addresses"
}

first_release="false"
if [ -z "$previous" ] || [ ! -s "$previous" ]; then
  first_release="true"
fi

rows=""
json_rows=""

for entry in "${contracts[@]}"; do
  IFS='|' read -r name wasm category flag <<< "$entry"

  hash="$(hash_of "$current" "$wasm")"
  [ -n "$hash" ] || continue

  prev="$(hash_of "$previous" "$wasm")"
  address="$(address_of "$wasm")"
  migrated="$(migrated_of "$wasm")"

  if [ "$first_release" = "true" ] || [ -z "$prev" ] || [ "$hash" != "$prev" ]; then
    changed="true"
  else
    changed="false"
  fi

  state="unchanged"
  [ "$changed" = "true" ] && state="changed"

  prev_cell="$prev"
  [ -n "$prev_cell" ] || prev_cell="(none)"

  action="-"
  if [ "$mode" = "full" ] && [ -n "$address" ]; then
    action="deployed at \`$address\`"
  elif [ "$changed" = "true" ]; then
    case "$category" in
      router|app)
        action="\`ipsa tx contracts upgrade --wasm_hash $hash\` on the live contract"
        ;;
      light-client)
        if [ -n "$address" ] && [ -n "$migrated" ]; then
          action="deployed at \`$address\`; corridor \`$migrated\` migrated onto it"
        elif [ -n "$address" ]; then
          action="deployed at \`$address\`; \`ipsa tx contracts migrate-client ${flag:+$flag }--wasm_hash $hash\` to move clients onto it"
        else
          action="deployed with the live router as admin and its corridor migrated onto it"
        fi
        ;;
      code)
        action="republish and set on its app (\`set_voucher_wasm\` / \`set_account_wasm --wasm_hash $hash\`)"
        ;;
      inactive)
        action="not deployed by a release; deploy explicitly if it goes live"
        ;;
    esac
  fi

  rows="${rows}| $name | \`$hash\` | \`$prev_cell\` | $state | $action |
"

  json_rows="${json_rows}$(jq -nc \
    --arg name "$name" \
    --arg wasm "$wasm" \
    --arg category "$category" \
    --arg hash "$hash" \
    --arg previous "$prev" \
    --argjson changed "$changed" \
    --arg address "$address" \
    --arg migrated "$migrated" \
    --arg migrate_flag "$flag" \
    '{name: $name, wasm: $wasm, category: $category, hash: $hash,
      previous: (if $previous == "" then null else $previous end),
      changed: $changed,
      address: (if $address == "" then null else $address end),
      migrated_client_id: (if $migrated == "" then null else $migrated end),
      migrate_flag: (if $migrate_flag == "" then null else $migrate_flag end)}')
"
done

if [ -n "$md_out" ]; then
  {
    echo "## Release $tag on Stellar $network"
    echo
    if [ "$first_release" = "true" ]; then
      echo "First release: no previous release to compare against, so every contract counts as changed."
    else
      echo "Wasm hashes compared against the previous release. A hash is the sha256 of the wasm and moves on any change under \`src/\`, formatting included, so changed means different bytes, not necessarily different behaviour."
    fi
    echo
    echo "| Contract | Wasm hash | Previous | State | When changed |"
    echo "|---|---|---|---|---|"
    printf '%s' "$rows"
    echo
    if [ "$mode" = "full" ]; then
      echo "This release deployed a fresh full set of contracts and wired them. The addresses are in \`deployments.json\`."
    else
      router_note="the live router"
      [ -n "$router" ] && router_note="the live router \`$router\`"
      echo "This release deploys only the light clients whose hash changed, with $router_note as their admin, and migrates each changed corridor onto its new client. The router and apps are not deployed; upgrade them in place with the hashes above. See the repository README's Release section for the staging rollout order."
    fi
  } > "$md_out"
fi

if [ -n "$json_out" ]; then
  printf '%s' "$json_rows" | jq -s \
    --arg tag "$tag" \
    --arg network "$network" \
    --arg mode "$mode" \
    --arg router "$router" \
    --argjson first_release "$first_release" \
    '{tag: $tag, network: $network, mode: $mode,
      router: (if $router == "" then null else $router end),
      first_release: $first_release,
      contracts: .}' > "$json_out"
fi
