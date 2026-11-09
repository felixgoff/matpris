#!/usr/bin/env bash
# Sets up the GitHub secrets and variable the Release workflow needs, from this
# computer, with your own `gh` login. The keys are made here and stay here: nothing is
# printed, and only GitHub's encrypted secret store gets a copy.
#
#   bash scripts/setup-release-secrets.sh            # set what's missing
#   DRY_RUN=1 bash scripts/setup-release-secrets.sh  # show what it would do
#
# Needs: gh (logged in, with admin rights on the repo), bun, node, keytool (any JDK),
# openssl. On Windows run it from Git Bash.
#
# It never replaces an existing key: a new updater key or Android keystore would stop
# installed copies from updating (and the watch from talking to the phone). The keys
# are also kept in ~/.matpris/ — back that folder up (e.g. in your password manager).
set -euo pipefail

REPO=felixgoff/matpris
KEYS="$HOME/.matpris"
cd "$(dirname "$0")/.."

run() { if [ -n "${DRY_RUN:-}" ]; then echo "would run: $*"; else "$@"; fi; }
have_secret() { gh secret list --repo "$REPO" --json name --jq '.[].name' | grep -qx "$1"; }
have_variable() { gh variable list --repo "$REPO" --json name --jq '.[].name' | grep -qx "$1"; }
set_secret() { # name, value on stdin
	if [ -n "${DRY_RUN:-}" ]; then echo "would set secret $1"; cat >/dev/null; else gh secret set "$1" --repo "$REPO"; fi
}

for tool in gh bun node keytool openssl; do
	command -v "$tool" >/dev/null || { echo "Missing $tool. Install it and run this again." >&2; exit 1; }
done
gh auth status >/dev/null 2>&1 || { echo "Run gh auth login first." >&2; exit 1; }
mkdir -p "$KEYS" && chmod 700 "$KEYS"

# 1. The map: the MapKit key (.p8), from which every release signs its own token
#    (scripts/mapkit-token.mjs). Looked for in ~/.matpris, then Downloads (and copied
#    to ~/.matpris, where `bun run mapkit:token` finds it too).
# The newest file: a new key replaces one that was revoked.
p8=$(ls -t "$KEYS"/AuthKey_*.p8 2>/dev/null | head -1 || true)
if [ -z "$p8" ]; then
	found=$(ls -t "$HOME"/Downloads/AuthKey_*.p8 2>/dev/null | head -1 || true)
	if [ -n "$found" ]; then
		cp "$found" "$KEYS/" && chmod 600 "$KEYS/$(basename "$found")"
		p8="$KEYS/$(basename "$found")"
	fi
fi
if [ -n "$p8" ]; then
	key_id=$(basename "$p8" .p8 | sed 's/^AuthKey_//')
	set_secret MAPKIT_PRIVATE_KEY <"$p8"
	run gh variable set MAPKIT_KEY_ID --repo "$REPO" --body "$key_id"
	echo "✓ MapKit key $key_id set (MAPKIT_PRIVATE_KEY + MAPKIT_KEY_ID); releases sign their own map token"
elif have_secret PUBLIC_MAPKIT_TOKEN; then
	echo "✓ PUBLIC_MAPKIT_TOKEN is already set (no MapKit key found to replace it with)"
else
	echo "! No MapKit key: put AuthKey_<KEYID>.p8 in ~/.matpris or Downloads and run this again"
fi

# 2. Desktop updates: a Tauri updater key pair.
if have_secret TAURI_SIGNING_PRIVATE_KEY; then
	echo "✓ TAURI_SIGNING_PRIVATE_KEY is already set (not replaced)"
else
	key="$KEYS/updater.key"
	if [ ! -f "$key" ]; then
		# No line ending at all: on Windows openssl ends its output with \r\n, and gh strips
		# that from a secret, so a password kept with its \r wouldn't match the stored one.
		openssl rand -base64 24 | tr -d '\r\n' >"$KEYS/updater.password"
		chmod 600 "$KEYS/updater.password"
		run bunx tauri signer generate --ci -w "$key" -p "$(cat "$KEYS/updater.password")" >/dev/null
	fi
	set_secret TAURI_SIGNING_PRIVATE_KEY <"$key"
	set_secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD <"$KEYS/updater.password"
	if [ -n "${DRY_RUN:-}" ]; then
		echo "would set variable TAURI_UPDATER_PUBKEY"
	else
		gh variable set TAURI_UPDATER_PUBKEY --repo "$REPO" --body "$(cat "$key.pub")"
	fi
	echo "✓ Updater key made (~/.matpris/updater.key) and set as secret + TAURI_UPDATER_PUBKEY"
fi
if ! have_variable TAURI_UPDATER_PUBKEY && [ -f "$KEYS/updater.key.pub" ]; then
	run gh variable set TAURI_UPDATER_PUBKEY --repo "$REPO" --body "$(cat "$KEYS/updater.key.pub")"
fi

# 3. Android: one keystore for the phone and the watch (the Wear Data Layer only
#    connects apps signed with the same key).
if have_secret ANDROID_KEYSTORE; then
	echo "✓ ANDROID_KEYSTORE is already set (not replaced)"
else
	store="$KEYS/android-release.jks"
	if [ ! -f "$store" ]; then
		openssl rand -base64 24 | tr -d '\r\n/+=' >"$KEYS/android.password"
		chmod 600 "$KEYS/android.password"
		run keytool -genkeypair -keystore "$store" -storetype PKCS12 -alias matpris \
			-keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Matpris" \
			-storepass "$(cat "$KEYS/android.password")" -keypass "$(cat "$KEYS/android.password")"
	fi
	if [ -z "${DRY_RUN:-}" ]; then base64 <"$store" | tr -d '\n' | set_secret ANDROID_KEYSTORE; else echo "would set secret ANDROID_KEYSTORE"; fi
	set_secret ANDROID_KEYSTORE_PASSWORD <"$KEYS/android.password"
	printf 'matpris' | set_secret ANDROID_KEY_ALIAS
	set_secret ANDROID_KEY_PASSWORD <"$KEYS/android.password" # PKCS12: same as the store's
	echo "✓ Android keystore made (~/.matpris/android-release.jks) and set as secrets"
fi

echo
echo "Done. Back up ~/.matpris/: losing it means installed apps can't update to new releases."
