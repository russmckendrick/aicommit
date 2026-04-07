# Providers

V1 ships with these provider paths:

```text
openai
groq
deepseek
openrouter
aimlapi
ollama
test
```

`openai`, `groq`, `deepseek`, `openrouter`, and `aimlapi` use the OpenAI-compatible chat-completions wire format.

Configure OpenAI-compatible providers:

```sh
aic config set AIC_AI_PROVIDER=openai AIC_API_KEY=<key> AIC_MODEL=gpt-5.4-mini
```

The default OpenAI model is `gpt-5.4-mini`, the cost-efficient GPT-5.4 variant.

Use a custom compatible endpoint:

```sh
aic config set AIC_AI_PROVIDER=openai AIC_API_URL=https://example.com/v1
```

Configure Ollama:

```sh
aic config set AIC_AI_PROVIDER=ollama AIC_API_URL=http://localhost:11434 AIC_MODEL=mistral
```

List cached or fallback models:

```sh
aic models
aic models --refresh
aic models --provider ollama
```

The model cache is stored at `~/.aicommit-models.json` and uses a 7-day TTL.

Anthropic, Gemini, Mistral-native, Azure, and Flowise are represented as planned provider names but are not enabled in this v1 provider implementation.
