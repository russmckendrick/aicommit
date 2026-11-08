# Demo tapes

[`vhs`](https://github.com/charmbracelet/vhs) scripts for the terminal UI demos used in the README and `docs/tui.md`.

```sh
brew install vhs
vhs docs/tapes/commit.tape
vhs docs/tapes/split.tape
```

Each tape creates a throwaway repository with `demo-repo.sh`, so it never touches this one. The tapes use whatever provider your `aic` config points at; a fast model keeps the GIFs short.
