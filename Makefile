MARKDOWN_FILES := docs/execplans/codex-agent-support.md

.PHONY: markdownlint nixie

markdownlint:
	markdownlint-cli2 $(MARKDOWN_FILES)

nixie:
	nixie $(MARKDOWN_FILES)
