id          := "chs.deck.sdPlugin"
plugins_dir := "/home/chs/.config/opendeck/plugins"
profile_dir := "/home/chs/.config/opendeck/profiles/99-4250D2781310"

# Build release binary
build:
    cargo build --release --target x86_64-unknown-linux-gnu --target-dir target/plugin

# Deploy plugin files (no rebuild)
deploy:
    rm -rf {{plugins_dir}}/{{id}}
    mkdir -p {{plugins_dir}}/{{id}}/assets
    cp manifest.json        {{plugins_dir}}/{{id}}/
    cp assets/*.png         {{plugins_dir}}/{{id}}/assets/
    cp assets/pi-*.html      {{plugins_dir}}/{{id}}/assets/
    cp screenshot.js         {{plugins_dir}}/{{id}}/
    cp target/plugin/x86_64-unknown-linux-gnu/release/chs-deck {{plugins_dir}}/{{id}}/chs-deck-linux
    @echo "Deployed to {{plugins_dir}}/{{id}}"

# Build + deploy
install: build deploy

# Generate one profile JSON from profiles/<name>.toml  (default: work)
gen-profile name="work":
    cargo run --bin gen_profile -- {{name}} 2>/dev/null > {{profile_dir}}/{{name}}.json
    @echo "Profile '{{name}}' written"

# Regenerate ALL profiles — удаляет устаревшие JSON, пишет актуальные
gen-profiles:
    #!/usr/bin/env bash
    set -e
    # Удаляем JSON-файлы без соответствующего .toml
    for j in {{profile_dir}}/*.json; do
      name=$(basename "$j" .json)
      if [ ! -f "profiles/$name.toml" ]; then
        rm "$j"
        echo "  ✗ removed $name"
      fi
    done
    # Генерируем актуальные
    for f in profiles/*.toml; do
      name=$(basename "$f" .toml)
      cargo run --bin gen_profile -- "$name" 2>/dev/null > "{{profile_dir}}/$name.json"
      echo "  ✓ $name"
    done
    echo "Done → {{profile_dir}}"

# Regenerate все профили + перезапуск OpenDeck
reload: gen-profiles
    pkill -x opendeck || true
    sleep 1
    opendeck &
    @echo "OpenDeck restarted"

# Генерация всех профилей + перезапуск OpenDeck
reload-profile: gen-profiles
    pkill -x opendeck || true
    sleep 1
    opendeck &

# Full cycle: build + deploy + reload profile
release: install reload-profile

package: build collect zip

collect:
    rm -rf build
    mkdir -p build/{{id}}
    cp -r assets build/{{id}}
    cp manifest.json build/{{id}}
    cp target/plugin/x86_64-unknown-linux-gnu/release/chs-deck build/{{id}}/chs-deck-linux

[working-directory: "build"]
zip:
    zip -r chs-deck.plugin.zip {{id}}/
