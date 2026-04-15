# Gate ⛩️

A domain-specific language for version control workflows.

Gate replaces Makefiles, bash scripts and CI/CD YAML for local automation. It understands version control operations natively — `save`, `sync`, `mirror`, `snapshot` are first-class citizens, not shell commands wrapped in string.

```gate
workflow release() {
    version = semver.bump(minor)
    confirmed = confirm("Deploy {version} to production?")

    snapshot.create("before-release")

    for platform in [github, gitlab, codeberg] {
        sync(push: true)
    }

    tag.release(version)
    mirror.sync()
    notify("Release {version} complete")

    on_error {
        snapshot.restore("before-release")
        notify("Release failed — reverted")
    }
}
```

---

## Why Gate?

| | Makefile | GitHub Actions | Bash | Gate |
|---|---|---|---|---|
| Readable without a manual | ❌ | ⚠️ | ❌ | ✅ |
| VCS operations as primitives | ❌ | ❌ | ❌ | ✅ |
| Multi-platform support | ❌ | ❌ | ❌ | ✅ |
| Built-in secret scanning | ❌ | ❌ | ❌ | ✅ |
| AI-friendly syntax | ❌ | ⚠️ | ❌ | ✅ |
| No configuration needed | ✅ | ❌ | ✅ | ✅ |
| Parallel execution | ❌ | ✅ | ⚠️ | ✅ |

Gate is the closest thing to **what Makefile would look like if it had been designed for modern developers** working across multiple platforms and with AI agents generating code.

---

## Requirements

- [Torii](https://gitorii.com) — Gate executes torii commands under the hood

---

## Install

```bash
cargo install gate-lang
```

---

## Language overview

### Workflows

```gate
workflow deploy(message, env = "prod") {
    save(message)
    sync(push: true)
}
```

### Variables

```gate
version = "1.0.0"
message = input("Commit message")
confirmed = confirm("Deploy to prod?")
```

### String interpolation

```gate
print("Deploying version {version} to {env}")
```

### Control flow

```gate
if env == "prod" {
    confirm("Are you sure?")
}

for platform in [github, gitlab] {
    sync(push: true)
}
```

### Error handling

```gate
workflow deploy() {
    sync(push: true)

    on_error {
        snapshot.restore("before-deploy")
        notify("Deploy failed")
    }
}
```

### Async / parallel execution

```gate
workflow deploy_all() {
    futures = []

    for platform in platforms {
        f = async sync(push: true)
        futures.push(f)
    }

    await all(futures) timeout 2m
    notify("All platforms synced")

    on_timeout {
        notify("Sync timed out after 2m")
    }
}
```

### Structs

```gate
struct Config {
    env: string = "prod",
    platforms: list = [github, gitlab]
}

workflow deploy(config: Config) {
    for platform in config.platforms {
        sync(push: true)
    }
}
```

### Enums

```gate
enum Env {
    dev,
    staging,
    prod
}

workflow deploy(env: Env) {
    if env == Env.prod {
        confirm("Deploy to production?")
    }
    sync(push: true)
}
```

### Imports

```gate
import "shared/notify.gate"

workflow release() {
    deploy()
    notify("Release complete")
}
```

### Notifications

```gate
// configure channels once
notify.channel("slack", "#deploys")
notify.channel("discord", "#dev-ops")

// send from any workflow
notify("Deploy complete")           // all channels
notify.to("slack", "Deploy done")   // specific channel
```

---

## Native functions

### Core
| Function | Description |
|----------|-------------|
| `save(message)` | Commit staged changes |
| `sync()` | Pull and push |
| `sync(push: true)` | Push only |

### Snapshots
| Function | Description |
|----------|-------------|
| `snapshot.create(name)` | Create a local snapshot |
| `snapshot.restore(name)` | Restore from snapshot |
| `snapshot.list()` | List snapshots |

### Tags & versions
| Function | Description |
|----------|-------------|
| `tag.release(version)` | Create a release tag |
| `semver.bump(minor)` | Bump semantic version |

### Mirrors
| Function | Description |
|----------|-------------|
| `mirror.sync()` | Sync all mirrors |
| `mirror.add(platform, repo)` | Add a mirror |

### I/O
| Function | Description |
|----------|-------------|
| `print(message)` | Print to stdout |
| `input(prompt)` | Read user input |
| `confirm(prompt)` | Yes/no prompt — halts on no |
| `notify(message)` | Send to all configured channels |
| `notify.to(channel, message)` | Send to specific channel |

---

## Type system

| Type | Example |
|------|---------|
| `string` | `"hello"` |
| `number` | `42`, `3.14` |
| `bool` | `true`, `false` |
| `null` | `null` |
| `list` | `[github, gitlab]` |
| `map` | `{key: "value"}` |
| `struct` | `Config { env: "prod" }` |
| `enum` | `Env.prod` |
| `duration` | `30s`, `5m`, `1h`, `7d` |
| `date` | `2026-05-01` |
| `datetime` | `2026-05-01T10:00:00` |
| `future` | result of `async` |
| `channel` | for concurrent workflows |
| `version` | `1.0.0` |
| `path` | `./deploy.gate` |
| `url` | `https://...` |
| `regex` | `/feat:.*/` |

---

## Full spec

See [gate-spec.md](gate-spec.md) for the complete language specification.

---

## License

Apache 2.0 — see [LICENSE](LICENSE).

Built as part of the [Torii](https://gitorii.com) ecosystem.
