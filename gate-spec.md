# Gate Language Specification v0.1

Gate is a domain-specific language for version control workflows. It is the native automation language of the Torii ecosystem.

**File extension:** `.gate`

---

## Philosophy

- Human-readable — any developer understands it without a manual
- Designed for VCS operations, not general purpose programming
- No semicolons, minimal boilerplate
- Concurrency as a first-class citizen
- Safe by default — `on_error` and `on_timeout` blocks prevent silent failures

---

## Type System

### Primitives
```gate
string    // "hello"
number    // 42, 3.14
bool      // true, false
null      // absence of value
```

### Collections
```gate
list      // [github, gitlab, codeberg]
map       // {key: "value", count: 3}
```

### Structs
```gate
struct Deploy {
    message: string,
    env: string = "prod",
    platforms: list = [github, gitlab]
}

impl Deploy {
    fn summary() {
        print("Deploying to {self.env}: {self.message}")
    }
}
```

### Domain types
```gate
version   // 1.0.0 — semver aware
path      // ./deploy.gate — filesystem path
url       // https://hooks.slack.com/... — validated URL
regex     // /feat:.*/ — pattern matching
bytes     // raw binary content
```

### Time types
```gate
date      // 2026-05-01
datetime  // 2026-05-01T10:00:00
duration  // 30s, 5m, 1h, 7d
```

### Concurrency types
```gate
future    // result of an async operation
channel   // communication between concurrent workflows
```

### Enums
```gate
enum Platform {
    github,
    gitlab,
    codeberg,
    gitea,
    forgejo
}

enum Env {
    dev,
    staging,
    prod
}
```

---

## Variables

```gate
// assignment
message = "feat: new feature"
env = "prod"

// typed
release_date: date = 2026-05-01
timeout: duration = 30m
platform: Platform = Platform.github

// from user input
message = input("Commit message")
confirmed = confirm("Deploy to prod?")

// null
result = null
```

---

## String interpolation

```gate
version = "1.0.0"
print("Release {version} complete")
notify("Deployed to {env} at {datetime.now()}")
```

---

## Control flow

### Conditionals
```gate
if env == "prod" {
    confirm("Deploy to production?")
}

if version > 1.0.0 {
    tag.release(version)
} else {
    print("Version too low")
}
```

### Loops
```gate
for platform in platforms {
    sync(push: true)
}

for repo in repos {
    if repo.branch == "main" {
        save("chore: update")
    }
}
```

---

## Workflows

### Basic
```gate
workflow deploy(message, env = "prod") {
    save(message)
    sync(push: true)
}
```

### With return value
```gate
workflow get_version() {
    return semver.bump(minor)
}
```

### Calling other workflows
```gate
workflow release() {
    version = get_version()
    deploy("chore: release {version}")
    tag.release(version)
}
```

### Error handling
```gate
workflow deploy(message) {
    save(message)
    sync(push: true)

    on_error {
        print("Deploy failed, reverting...")
        snapshot.restore("before-deploy")
        notify("Deploy failed")
    }
}
```

---

## Concurrency

### Async / await
```gate
workflow deploy_all() {
    futures = []

    for platform in platforms {
        f = async deploy(platform)
        futures.push(f)
    }

    await all(futures)
    notify("All platforms deployed")
}
```

### With timeout
```gate
result = await sync(push: true) timeout 30s

on_timeout {
    notify.to("slack", "Sync timed out after 30s")
}
```

### Channels
```gate
ch: channel = channel.new()

workflow producer() {
    ch.send("ready")
}

workflow consumer() {
    msg = ch.receive()
    print("Received: {msg}")
}
```

---

## Imports

```gate
import "shared/deploy.gate"
import "shared/notify.gate"

workflow release(version) {
    deploy(version)
    notify("Release {version} complete")
}
```

---

## I/O and interaction

```gate
print("mensaje")                    // output to stdout
input("Commit message")             // prompt user for input
confirm("Deploy to prod?")          // yes/no prompt, halts on no
```

---

## Notifications

Channels are configured globally, workflows just send:

```gate
// configuration (torii config or .gate config file)
notify.channel("slack", "#deploys")
notify.channel("discord", "#dev-ops")
notify.channel("email", "team@company.com")

// in workflows
notify("Deploy complete")               // sends to all configured channels
notify.to("slack", "Deploy complete")   // sends to specific channel
```

---

## Native Torii functions

```gate
// Core
save(message)
save(message, files: [path])
sync()
sync(push: true)
sync(pull: true)
sync(force: true)

// Branches
branch.create("feature-x")
branch.switch("main")
branch.delete("old-feature")
branch.list()

// Tags & versions
tag.create("v1.0.0")
tag.release(version)
semver.bump(major)
semver.bump(minor)
semver.bump(patch)

// Snapshots
snapshot.create("before-deploy")
snapshot.restore("before-deploy")
snapshot.list()

// Mirrors
mirror.sync()
mirror.add(platform, repo)

// Remote
remote.create(platform, name)
remote.delete(platform, name)

// Scanner
scan.staged()
scan.history()

// History
history.reflog()
history.rewrite(start, end)

// Repo info
repo.current_branch()
repo.status()
repo.last_commit()
```

---

## Comments

```gate
// single line

/*
    multi
    line
*/
```

---

## Complete example

```gate
import "shared/notify.gate"

enum Env {
    staging,
    prod
}

struct ReleaseConfig {
    message: string,
    env: Env = Env.staging,
    platforms: list = [github, gitlab],
    notify_channel: string = "slack"
}

workflow get_next_version() {
    return semver.bump(minor)
}

workflow deploy(config: ReleaseConfig) {
    snapshot.create("before-deploy")

    futures = []
    for platform in config.platforms {
        f = async sync(push: true)
        futures.push(f)
    }

    await all(futures) timeout 2m

    notify.to(config.notify_channel, "Deployed to {config.env}")

    on_error {
        snapshot.restore("before-deploy")
        notify.to(config.notify_channel, "Deploy failed, reverted")
    }

    on_timeout {
        notify.to(config.notify_channel, "Deploy timed out after 2m")
    }
}

workflow release() {
    confirmed = confirm("Deploy to production?")

    version = get_next_version()

    config = ReleaseConfig {
        message: "chore: release {version}",
        env: Env.prod,
        platforms: [github, gitlab, codeberg]
    }

    deploy(config)
    tag.release(version)

    notify("Release {version} complete")
}
```
