#!/usr/bin/env bash
# Verify downloaded CLI/GUI bundles before uploading or publishing a release.
set -euo pipefail

artifact_dir=${1:-artifacts}
cd "$artifact_dir"

errors=0
cli_archives=(
  geezipx-linux-x86_64.tar.gz
  geezipx-macos-x86_64.tar.gz
  geezipx-windows-x86_64.zip
)
assets=("${cli_archives[@]}")

for asset in "${cli_archives[@]}"; do
  if [[ ! -f "$asset" || ! -s "$asset" || ! -s "$asset.sha256" ]]; then
    echo "::error::$asset or its checksum is missing/empty"
    errors=$((errors + 1))
  elif ! sha256sum --check --strict "$asset.sha256"; then
    echo "::error::$asset checksum verification failed"
    errors=$((errors + 1))
  fi
done

for pattern in '*.AppImage' '*.deb' '*.dmg' '*-setup.exe'; do
  mapfile -t matches < <(compgen -G "$pattern" || true)
  if [[ ${#matches[@]} -eq 0 ]]; then
    echo "::error::Required GUI bundle missing: $pattern"
    errors=$((errors + 1))
    continue
  fi
  for asset in "${matches[@]}"; do
    if [[ ! -f "$asset" || ! -s "$asset" ]]; then
      echo "::error::GUI bundle is missing/empty: $asset"
      errors=$((errors + 1))
    fi
    assets+=("$asset")
  done
done

if [[ "$errors" -gt 0 ]]; then
  echo "::error::$errors release artifact check(s) failed"
  exit 1
fi

# Only write checksums after every required artifact and original CLI checksum
# has passed. Include GUI bundles in individual and combined checksum files.
for asset in "${assets[@]}"; do
  sha256sum -- "$asset" > "$asset.sha256"
done
sha256sum -- "${assets[@]}" > SHA256SUMS
sha256sum --check --strict SHA256SUMS

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    printf '## Release artifact verification\n\n'
    printf '| Artifact | Bytes | SHA256 |\n| --- | ---: | --- |\n'
    for asset in "${assets[@]}"; do
      checksum=$(sha256sum -- "$asset")
      size=$(wc -c < "$asset")
      printf '| `%s` | %s | `%s` |\n' "$asset" "$size" "${checksum%% *}"
    done
    printf '\nAll required CLI and GUI bundles passed. Individual checksums and `SHA256SUMS` are included in `geezipx-release-artifacts`.\n'
  } >> "$GITHUB_STEP_SUMMARY"
fi

echo "Verified ${#assets[@]} release artifacts"
