# Alpine Linux qcow2 Image Build
# Reproducible QEMU-bootable image for InfraSim
#
# Usage:
#   make build        - Build the Alpine qcow2 image
#   make boot-test    - Boot-test the image in headless QEMU
#   make bundle       - Create the signed .tar.gz bundle
#   make snapshot     - Create a snapshot release bundle
#   make clean        - Remove build artifacts
#
# CI Usage:
#   make ci-build     - Full CI build pipeline
#   make ci-snapshot  - Create snapshot for GitHub release

ALPINE_VERSION := 3.20
ALPINE_ARCH := aarch64
ALPINE_MIRROR := https://dl-cdn.alpinelinux.org/alpine
ALPINE_RELEASE := v$(ALPINE_VERSION)

IMAGE_SIZE := 2G
OUTPUT_DIR := $(CURDIR)/output
DISK_FILE := $(OUTPUT_DIR)/base.qcow2
BUNDLE_DIR := $(CURDIR)/../../dist
SNAPSHOTS_DIR := $(CURDIR)/../../snapshots

GIT_SHA := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_SHA_FULL := $(shell git rev-parse HEAD 2>/dev/null || echo "unknown")
BUILD_DATE := $(shell date -u +"%Y-%m-%dT%H:%M:%SZ")
VERSION := $(shell git describe --tags --always 2>/dev/null || echo "dev-$(GIT_SHA)")

# CI environment detection
CI ?= false
GITHUB_RUN_ID ?=

.PHONY: all build boot-test bundle snapshot clean ci-build ci-snapshot

all: build

$(OUTPUT_DIR):
	mkdir -p $(OUTPUT_DIR)/logs $(OUTPUT_DIR)/meta

$(SNAPSHOTS_DIR):
	mkdir -p $(SNAPSHOTS_DIR)

build: $(OUTPUT_DIR)
	CI=$(CI) GITHUB_RUN_ID=$(GITHUB_RUN_ID) ./build-alpine-qcow2.sh \
		--version $(ALPINE_VERSION) \
		--arch $(ALPINE_ARCH) \
		--size $(IMAGE_SIZE) \
		--output $(DISK_FILE)

boot-test: $(DISK_FILE)
	./boot-test.sh $(DISK_FILE)

bundle: build
	../../ci/bundle_qcow2.sh \
		--input $(OUTPUT_DIR) \
		--output $(BUNDLE_DIR)/infrasim-alpine-$(GIT_SHA).tar.gz \
		--version $(ALPINE_VERSION) \
		--git-sha $(GIT_SHA)

snapshot: build $(SNAPSHOTS_DIR)
	@echo "Creating snapshot: infrasim-alpine-$(VERSION)"
	../../ci/bundle_qcow2.sh \
		--input $(OUTPUT_DIR) \
		--output $(SNAPSHOTS_DIR)/infrasim-alpine-$(VERSION).tar.gz \
		--version $(ALPINE_VERSION) \
		--git-sha $(GIT_SHA)
	@echo "Snapshot created: $(SNAPSHOTS_DIR)/infrasim-alpine-$(VERSION).tar.gz"

# CI targets
ci-build: build boot-test
	@echo "CI build complete"
	@echo "Image: $(DISK_FILE)"
	@echo "SHA: $(GIT_SHA)"

ci-snapshot: ci-build snapshot
	@echo "CI snapshot complete"
	@ls -la $(SNAPSHOTS_DIR)/

clean:
	rm -rf $(OUTPUT_DIR)
