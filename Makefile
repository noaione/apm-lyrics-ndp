.PHONY: build clean

TARGET := wasm32-wasip1
PLUGIN_NAME := apm-lyrics-ndp

build:
	cargo build --release --target $(TARGET)
	mkdir -p bundle
	cp manifest.json bundle/
	cp target/$(TARGET)/release/*.wasm bundle/plugin.wasm
	cd bundle && zip -r ../$(PLUGIN_NAME).ndp .
	rm -rf bundle

clean:
	rm -rf bundle $(PLUGIN_NAME).ndp
