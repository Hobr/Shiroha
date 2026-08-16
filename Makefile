GO_GET := go get -u ./...
GO_TIDY := go mod tidy

MODULES := \
	internal/ir \
	cmd/daemon

daemon:
	go run -v ./cmd/daemon

daemon-build:
	go build -o target/Daemon -v ./cmd/daemon

init: install update

install:
	go env -w GO111MODULE=on
	go env -w GOPROXY=https://goproxy.cn,direct
	go work init
	go work use -r .

update:
	@for mod in $(MODULES); do \
		echo "Updating $$mod"; \
		(cd $$mod && $(GO_GET) && $(GO_TIDY)); \
	done
	go work sync

.PHONY: init daemon daemon-build update
