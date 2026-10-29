#!/bin/sh
# Compiler Moli sur un Mac (ou tout poste avec rustup, zig et Node), pour la machine qui le fera tourner.
#
#   sh scripts/build-mac.sh [cible]
#
# <cible> : la machine qui fera tourner le programme (défaut : x86_64-unknown-linux-musl, un NAS Intel/AMD).
#   x86_64-unknown-linux-musl    NAS Intel ou AMD (Synology DS923+…)
#   aarch64-unknown-linux-musl   NAS ou carte ARM (Raspberry Pi, Synology ARM…)
#   native                       le Mac lui-même, pour l'essayer en local
#
# Le tableau de bord (web/) est construit d'abord : il est embarqué dans le programme.
# Le résultat est écrit en dernière ligne : le chemin du programme.
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$HOME/.local/zig:$HOME/.local/node/bin:$PATH"
cible=${1:-x86_64-unknown-linux-musl}

echo "1/2 Tableau de bord"
(cd web && npm ci --no-audit --no-fund >/dev/null && npm run build >/dev/null)

echo "2/2 Programme pour $cible"
if [ "$cible" = native ]; then
  cargo build --release --locked -p moli-os
  echo "target/release/moli-os"
else
  rustup target add "$cible" >/dev/null 2>&1 || true
  cargo zigbuild --release --locked -p moli-os --target "$cible"
  echo "target/$cible/release/moli-os"
fi
