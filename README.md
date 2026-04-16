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

### Save

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `save(message)` | `torii save -m "msg"` | Commit staged changes |
| `save(message, all: true)` | `torii save -am "msg"` | Stage all and commit |
| `save(message, files: [...])` | `torii save <files> -m "msg"` | Stage specific files and commit |
| `save(message, amend: true)` | `torii save --amend -m "msg"` | Amend last commit |
| `save.revert(hash)` | `torii save --revert <hash> -m "..."` | Revert a commit |
| `save.reset(hash, mode: "soft")` | `torii save --reset <hash> --reset-mode soft` | Reset to commit |

### Sync

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `sync()` | `torii sync` | Pull and push |
| `sync(push: true)` | `torii sync --push` | Push only |
| `sync(pull: true)` | `torii sync --pull` | Pull only |
| `sync(force: true)` | `torii sync --force` | Force push |
| `sync(fetch: true)` | `torii sync --fetch` | Fetch without merging |
| `sync(branch: "name")` | `torii sync <branch>` | Integrate branch (smart merge/rebase) |
| `sync(branch: "name", merge: true)` | `torii sync <branch> --merge` | Force merge strategy |
| `sync(branch: "name", rebase: true)` | `torii sync <branch> --rebase` | Force rebase strategy |
| `sync(branch: "name", preview: true)` | `torii sync <branch> --preview` | Preview without executing |

### Branch

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `branch.list()` | `torii branch --list` | List local branches |
| `branch.list(all: true)` | `torii branch --all` | List local and remote branches |
| `branch.create(name)` | `torii branch <name> -c` | Create and switch to branch |
| `branch.switch(name)` | `torii branch <name>` | Switch to branch |
| `branch.delete(name)` | `torii branch -d <name>` | Delete branch |
| `branch.rename(name)` | `torii branch --rename <name>` | Rename current branch |

### Inspect

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `ls()` | `torii ls` | List all tracked files |
| `ls(path)` | `torii ls <path>` | List tracked files under path |
| `show()` | `torii show` | Show HEAD commit with diff |
| `show(ref)` | `torii show <hash/tag>` | Show specific commit or tag |
| `show.blame(file)` | `torii show <file> --blame` | Line-by-line change history |
| `show.blame(file, lines: "10,20")` | `torii show <file> --blame -L 10,20` | Blame specific line range |

### Log

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `log()` | `torii log` | Last 10 commits |
| `log(count: 50)` | `torii log -n 50` | Last N commits |
| `log(oneline: true)` | `torii log --oneline` | Compact view |
| `log(author: "name")` | `torii log --author "name"` | Filter by author |
| `log(since: "2026-01-01")` | `torii log --since 2026-01-01` | Filter by date |
| `log(grep: "feat")` | `torii log --grep "feat"` | Filter by message |
| `log(stat: true)` | `torii log --stat` | Show file change stats |

### History

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `history.rebase(target)` | `torii history rebase <target>` | Rebase onto branch |
| `history.rebase(target, interactive: true)` | `torii history rebase <target> -i` | Interactive rebase |
| `history.rebase.continue()` | `torii history rebase --continue` | Continue rebase |
| `history.rebase.abort()` | `torii history rebase --abort` | Abort rebase |
| `history.cherry_pick(hash)` | `torii history cherry-pick <hash>` | Apply commit to current branch |
| `history.cherry_pick.continue()` | `torii history cherry-pick --continue` | Continue cherry-pick |
| `history.cherry_pick.abort()` | `torii history cherry-pick --abort` | Abort cherry-pick |
| `history.blame(file)` | `torii history blame <file>` | Line-by-line change history |
| `history.blame(file, lines: "10,20")` | `torii history blame <file> -L 10,20` | Blame specific lines |
| `history.scan()` | `torii history scan` | Scan staged files for secrets |
| `history.scan(all: true)` | `torii history scan --history` | Scan entire git history |
| `history.remove_file(path)` | `torii history remove-file <path>` | Purge file from all commits |
| `history.clean()` | `torii history clean` | GC + reflog expire |
| `history.reflog()` | `torii history reflog` | HEAD movement history |
| `history.verify_remote()` | `torii history verify-remote` | Verify remote state |
| `history.rewrite(start, end)` | `torii history rewrite <start> <end>` | Rewrite commit dates |

### Snapshots

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `snapshot.create(name)` | `torii snapshot create -n <name>` | Create a local snapshot |
| `snapshot.restore(id)` | `torii snapshot restore <id>` | Restore from snapshot |
| `snapshot.list()` | `torii snapshot list` | List snapshots |
| `snapshot.delete(id)` | `torii snapshot delete <id>` | Delete a snapshot |
| `snapshot.stash()` | `torii snapshot stash` | Stash current work |
| `snapshot.stash(untracked: true)` | `torii snapshot stash -u` | Stash including untracked files |
| `snapshot.unstash()` | `torii snapshot unstash` | Restore latest stash |
| `snapshot.unstash(id)` | `torii snapshot unstash <id>` | Restore specific stash |
| `snapshot.undo()` | `torii snapshot undo` | Undo last operation |

### Tags & versions

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `tag.create(name)` | `torii tag create <name>` | Create a tag |
| `tag.create(name, message: "msg")` | `torii tag create <name> -m "msg"` | Create annotated tag |
| `tag.list()` | `torii tag list` | List all tags |
| `tag.delete(name)` | `torii tag delete <name>` | Delete a tag |
| `tag.push(name)` | `torii tag push <name>` | Push specific tag |
| `tag.push()` | `torii tag push` | Push all tags |
| `tag.show(name)` | `torii tag show <name>` | Show tag details |
| `tag.release()` | `torii tag release` | Auto-bump from conventional commits |
| `tag.release(bump: "minor")` | `torii tag release --bump minor` | Force bump type |
| `semver.bump(minor)` | `torii tag release --bump minor` | Bump semantic version |

### Mirrors

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `mirror.sync()` | `torii mirror sync` | Sync all mirrors |
| `mirror.sync(force: true)` | `torii mirror sync --force` | Force sync |
| `mirror.list()` | `torii mirror list` | List mirrors |
| `mirror.add_master(platform, account, repo)` | `torii mirror add-master <platform> user <account> <repo>` | Add master mirror |
| `mirror.add_slave(platform, account, repo)` | `torii mirror add-slave <platform> user <account> <repo>` | Add slave mirror |
| `mirror.remove(platform, account)` | `torii mirror remove <platform> <account>` | Remove mirror |
| `mirror.set_master(platform, account)` | `torii mirror set-master <platform> <account>` | Set new master |
| `mirror.autofetch(enable: true, interval: "30m")` | `torii mirror autofetch --enable --interval 30m` | Configure autofetch |

### Remote

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `remote.create(platform, name)` | `torii remote create <platform> <name>` | Create remote repo |
| `remote.delete(platform, owner, repo)` | `torii remote delete <platform> <owner> <repo>` | Delete remote repo |
| `remote.visibility(platform, owner, repo, public: true)` | `torii remote visibility <platform> <owner> <repo> --public` | Change visibility |
| `remote.info(platform, owner, repo)` | `torii remote info <platform> <owner> <repo>` | Show repo info |
| `remote.list(platform)` | `torii remote list <platform>` | List repos on platform |
| `repo.create(name, platforms: [...])` | `torii repo <name> --platforms github,gitlab --create` | Batch create across platforms |
| `repo.delete(name, platforms: [...])` | `torii repo <name> --platforms github,gitlab --delete` | Batch delete across platforms |

### Config

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `config.set(key, value)` | `torii config set <key> <value>` | Set global config |
| `config.set(key, value, local: true)` | `torii config set <key> <value> --local` | Set local config |
| `config.get(key)` | `torii config get <key>` | Get config value |
| `config.list()` | `torii config list` | List all config |

### I/O

| Function | Description |
|----------|-------------|
| `print(message)` | Print to stdout |
| `input(prompt)` | Read user input |
| `confirm(prompt)` | Yes/no prompt — halts on no |
| `notify(message)` | Send to all configured channels |
| `notify.to(channel, message)` | Send to specific channel |
| `notify.channel(name, target)` | Configure notification channel |

### Utilities

| Function | Torii equivalent | Description |
|----------|-----------------|-------------|
| `clone(url)` | `torii clone <url>` | Clone a repository |
| `clone(platform, user_repo)` | `torii clone github user/repo` | Clone with platform shorthand |
| `ssh.check()` | `torii ssh-check` | Verify SSH setup |

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
