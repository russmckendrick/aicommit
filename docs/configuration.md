# Configuration

`aicommit` reads configuration in this order:

1. Built-in defaults
2. Global config at `~/.aicommit`
3. Local `.env`
4. Process environment variables

Set global values:

```sh
aic config set AIC_API_KEY=<key> AIC_MODEL=gpt-4o-mini
```

Read values:

```sh
aic config get AIC_MODEL AIC_AI_PROVIDER
```

Describe settings:

```sh
aic config describe
aic config describe AIC_MODEL
```

Supported v1 keys:

```text
AIC_AI_PROVIDER
AIC_API_KEY
AIC_API_URL
AIC_API_CUSTOM_HEADERS
AIC_PROXY
AIC_TOKENS_MAX_INPUT
AIC_TOKENS_MAX_OUTPUT
AIC_DESCRIPTION
AIC_EMOJI
AIC_MODEL
AIC_LANGUAGE
AIC_MESSAGE_TEMPLATE_PLACEHOLDER
AIC_ONE_LINE_COMMIT
AIC_OMIT_SCOPE
AIC_GITPUSH
AIC_HOOK_AUTO_UNCOMMENT
```

Example local `.env`:

```env
AIC_AI_PROVIDER=openai
AIC_MODEL=gpt-4o-mini
AIC_DESCRIPTION=false
AIC_EMOJI=false
```

Use `.aicommitignore` in a repository to exclude files from AI diff input:

```ignorelang
path/to/large-asset.zip
**/*.jpg
```
