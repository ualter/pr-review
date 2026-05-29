.DEFAULT_GOAL := help
SHELL = /usr/bin/env bash -o pipefail
.SHELLFLAGS = -ec

repo_path := /home/ualter/developer/repos/codecommit/datahub-code/datahub-backend-dev/

help:  ## Display this help
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-18s\033[33m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)


##@ --| BUILD  |--------------------------------------------------------------------------------------------------------------------------------------

build:  ## Build the project
	cargo build --release

review-pr: ## Review a CodeCommit Pull Request with GitHub Copilot CLI
	cargo run --release pr 4663 --run-copilot --repo-path $(repo_path)

review-commit: ## Review a CodeCommit Commit with GitHub Copilot CLI
	cargo run --release commit 5ceeb10f2c37a9d85d6ce26ba31d0f080e352603 --run-copilot --repo-path $(repo_path)
