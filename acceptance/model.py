from dataclasses import dataclass, replace


STATE_LABELS = (
    ("unknown", "待判断"),
    ("in_progress", "进行中"),
    ("review", "待审阅"),
    ("blocked", "已阻塞"),
    ("to_archive", "待归档"),
    ("completed", "已完成"),
)
PHASES = {
    "running": "in_progress",
    "approval": "blocked",
    "input": "blocked",
    "error": "blocked",
    "finished": "review",
}
EVENTS = (
    *("manual:" + state for state, _ in STATE_LABELS),
    *PHASES,
    "view",
    "restart",
)


@dataclass(frozen=True)
class Flow:
    state: str = "in_progress"
    phase: str = "running"
    manual: bool = False
    note: str = ""
    version: int = 1

    def key(self):
        return self.state, self.phase, self.manual, self.note

    def step(self, event):
        if event.startswith("manual:"):
            return replace(
                self, state=event.split(":", 1)[1], manual=True,
                note="人工备注", version=self.version + 1,
            )
        if event in {"view", "restart"} or event == self.phase:
            return self
        return replace(
            self, state=PHASES[event], phase=event, manual=False,
            version=self.version + 1,
        )


def reachable_paths():
    paths = {Flow().key(): ()}
    queue = [(Flow(), ())]
    for model, path in queue:
        for event in EVENTS:
            following = model.step(event)
            if following.key() not in paths:
                next_path = (*path, event)
                paths[following.key()] = next_path
                queue.append((following, next_path))
    return queue
