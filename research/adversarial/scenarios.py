import uuid
import random
from datetime import datetime, timedelta

def build_event(feat_name, fpath, branch, ctime, msg="", ticket_id="", pr_id=""):
    return {
        "event_id": str(uuid.uuid4()),
        "timestamp": ctime.timestamp(),
        "file_path": fpath,
        "branch": branch,
        "commit_msg": msg,
        "ticket_id": ticket_id,
        "pr_id": pr_id,
        "ground_truth": feat_name
    }

def scenario_a_shared_infrastructure(num_events=1000):
    """
    Scenario A: Shared Infrastructure
    auth.ts, logger.ts, config.ts used by every feature constantly over time.
    """
    events = []
    features = [
        {"name": "Auth", "files": ["jwt.ts", "login.ts"], "branch": "feat/auth"},
        {"name": "Payments", "files": ["invoice.ts", "stripe.ts"], "branch": "feat/pay"},
        {"name": "Search", "files": ["index.ts", "query.ts"], "branch": "feat/search"}
    ]
    shared_files = ["logger.ts", "config.ts", "auth.ts"]
    
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    for _ in range(num_events):
        feat = random.choice(features)
        current_time += timedelta(minutes=random.randint(1, 5))
        
        # 30% chance to touch a shared infrastructure file instead of a feature-specific file
        if random.random() < 0.3:
            fpath = random.choice(shared_files)
        else:
            fpath = random.choice(feat["files"])
            
        events.append(build_event(feat["name"], fpath, feat["branch"], current_time))
    return events

def scenario_b_extreme_context_switching(num_events=1000):
    """
    Scenario B: Extreme Context Switching
    Bouncing between Auth, Payments, and Hotfix every 15-20 events.
    """
    events = []
    features = [
        {"name": "Auth", "files": ["auth.ts"], "branch": "feat/auth"},
        {"name": "Payments", "files": ["pay.ts"], "branch": "feat/pay"},
        {"name": "Hotfix", "files": ["memory.ts"], "branch": "main"}
    ]
    
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    active_feat = random.choice(features)
    
    for i in range(num_events):
        # Switch every 15 to 20 events
        if i % random.randint(15, 20) == 0:
            active_feat = random.choice(features)
            # simulate gap for context switch
            current_time += timedelta(minutes=random.randint(30, 120))
            
        current_time += timedelta(minutes=random.randint(1, 3))
        fpath = random.choice(active_feat["files"])
        events.append(build_event(active_feat["name"], fpath, active_feat["branch"], current_time))
    return events


def scenario_c_missing_metadata(num_events=1000):
    """
    Scenario C: Missing Metadata
    Randomly remove branch and commit messages from 30-50% of events.
    """
    events = []
    features = [
        {"name": "Auth", "files": ["auth.ts", "jwt.ts"], "branch": "feat/auth"},
        {"name": "Payments", "files": ["pay.ts", "invoice.ts"], "branch": "feat/pay"}
    ]
    
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    for _ in range(num_events):
        feat = random.choice(features)
        current_time += timedelta(minutes=random.randint(1, 5))
        
        branch = feat["branch"]
        msg = f"Work on {feat['name']}"
        
        # 40% chance to lose branch data
        if random.random() < 0.4:
            branch = "main"
            
        # 50% chance to lose commit msg
        if random.random() < 0.5:
            msg = ""
            
        fpath = random.choice(feat["files"])
        events.append(build_event(feat["name"], fpath, branch, current_time, msg=msg))
    return events

def scenario_d_long_lived_feature(num_events=1000):
    """
    Scenario D: Long-Lived Feature
    Feature starts -> 2 weeks pass -> Feature resumes.
    """
    events = []
    
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    
    # Phase 1: Feature A starts (first 300 events)
    for _ in range(300):
        current_time += timedelta(minutes=random.randint(1, 10))
        events.append(build_event("LongFeature", "core.ts", "feat/long", current_time))
        
    # Phase 2: 2 weeks pass, 400 events of unrelated work
    current_time += timedelta(days=14)
    for _ in range(400):
        current_time += timedelta(minutes=random.randint(1, 10))
        events.append(build_event("OtherWork", "other.ts", "feat/other", current_time))
        
    # Phase 3: Feature A resumes (last 300 events)
    current_time += timedelta(days=2)
    for _ in range(300):
        current_time += timedelta(minutes=random.randint(1, 10))
        events.append(build_event("LongFeature", "core.ts", "feat/long", current_time))
        
    return events

def scenario_e_feature_twins(num_events=1000):
    """
    Scenario E: Feature Twins
    Two auth features with same file set, same branch naming style, same commit style.
    This intentionally removes obvious signals to test whether clustering overfits.
    """
    events = []
    features = [
        {
            "name": "Authentication Refactor",
            "files": ["auth.ts", "jwt.ts", "session.ts", "middleware.ts"],
        },
        {
            "name": "Authentication Migration",
            "files": ["auth.ts", "jwt.ts", "session.ts", "middleware.ts"],
        },
    ]
    # Shared branch pool across both features to avoid a branch-equality shortcut.
    branch_pool = ["feat/auth", "feat/auth-v2", "main"]
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active = random.choice(features)
    while remaining > 0:
        burst = min(remaining, random.randint(8, 22))
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            file_path = random.choice(active["files"])
            # Keep commit style similar on purpose.
            msg = random.choice(
                ["auth update", "auth cleanup", "auth improvement", "auth adjustment"]
            )
            branch = random.choice(branch_pool)
            events.append(build_event(active["name"], file_path, branch, current_time, msg=msg))
        remaining -= burst
        active = features[0] if active["name"] == features[1]["name"] else features[1]
        # Context switch gap between bursts.
        current_time += timedelta(minutes=random.randint(12, 40))
    return events

def scenario_f_monorepo(num_events=1000):
    """
    Scenario F: Monorepo
    Feature work spans frontend/backend/shared packages with occasional cross-feature overlap.
    """
    events = []
    features = [
        {
            "name": "Login Flow",
            "files": ["frontend/auth.ts", "backend/auth.ts", "shared/auth.ts"],
            "branch": "feat/login-flow",
        },
        {
            "name": "Payment Flow",
            "files": ["frontend/pay.ts", "backend/pay.ts", "shared/pay.ts"],
            "branch": "feat/payment-flow",
        },
    ]
    shared_noise = ["shared/config.ts", "shared/logger.ts"]
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    for _ in range(num_events):
        feat = random.choice(features)
        current_time += timedelta(minutes=random.randint(1, 5))
        # 20% touch shared monorepo utilities that blur boundaries.
        file_path = random.choice(shared_noise) if random.random() < 0.2 else random.choice(feat["files"])
        msg = random.choice(["sync modules", "shared contracts", "package update", "cross-package fix"])
        events.append(build_event(feat["name"], file_path, feat["branch"], current_time, msg=msg))
    return events

def scenario_g_giant_utility_file(num_events=1000):
    """
    Scenario G: Giant Utility File
    utils.ts appears in 90% of events (hub node pressure).
    """
    events = []
    features = [
        {"name": "Feature A", "files": ["a.ts"], "branch": "feat/a"},
        {"name": "Feature B", "files": ["b.ts"], "branch": "feat/b"},
        {"name": "Feature C", "files": ["c.ts"], "branch": "feat/c"},
    ]
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active = random.choice(features)
    while remaining > 0:
        burst = min(remaining, random.randint(10, 28))
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            # 90% touch utility hub file; 10% touch feature-local file.
            file_path = "utils.ts" if random.random() < 0.9 else active["files"][0]
            msg = random.choice(["refactor utility", "cleanup helpers", "utility patch"])
            # Force branch ambiguity to stress the utility hub.
            branch = random.choice(["main", "feat/mixed"])
            events.append(build_event(active["name"], file_path, branch, current_time, msg=msg))
        remaining -= burst
        # switch to another feature episode
        choices = [f for f in features if f["name"] != active["name"]]
        active = random.choice(choices)
        current_time += timedelta(minutes=random.randint(10, 35))
    return events

def scenario_e2_narrative_twins(num_events=1000):
    """
    Scenario E2: Narrative Twins
    Same files/branches for both features; only narrative sequence differs.
    """
    events = []
    shared_files = ["auth.ts", "jwt.ts", "session.ts"]
    branch_pool = ["feat/auth", "feat/auth-v2", "main"]

    narratives = {
        "Authentication Refactor": [
            "extract auth service",
            "remove legacy auth",
            "simplify middleware",
        ],
        "Authentication Migration": [
            "add oauth provider",
            "implement token exchange",
            "add sso callback",
        ],
    }

    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active = "Authentication Refactor"
    episode_idx = 0

    while remaining > 0:
        burst = min(remaining, random.randint(10, 25))
        commit_msg = narratives[active][episode_idx % len(narratives[active])]
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            file_path = random.choice(shared_files)
            branch = random.choice(branch_pool)
            events.append(build_event(active, file_path, branch, current_time, msg=commit_msg))

        remaining -= burst
        episode_idx += 1
        active = (
            "Authentication Migration"
            if active == "Authentication Refactor"
            else "Authentication Refactor"
        )
        current_time += timedelta(minutes=random.randint(10, 45))

    return events

def scenario_g2_narrative_hub(num_events=1000):
    """
    Scenario G2: Narrative Hub
    Shared utility hub file appears across all features, but in narrative-local episodes.
    """
    events = []
    features = [
        {
            "name": "Auth Feature",
            "anchor": "auth.ts",
            "narrative": ["auth hardening", "auth cleanup", "auth middleware"],
        },
        {
            "name": "Payments Feature",
            "anchor": "payments.ts",
            "narrative": ["payment retries", "invoice flow", "payment reconciliation"],
        },
        {
            "name": "Cache Feature",
            "anchor": "cache.ts",
            "narrative": ["cache policy", "cache invalidation", "cache tuning"],
        },
    ]

    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events

    while remaining > 0:
        feat = random.choice(features)
        burst = min(remaining, random.randint(12, 30))
        commit_msg = random.choice(feat["narrative"])
        for i in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            # Alternate between shared hub and feature anchor to create contextual cues.
            file_path = "utils.ts" if i % 2 == 0 else feat["anchor"]
            branch = random.choice(["main", "feat/mixed"])
            events.append(build_event(feat["name"], file_path, branch, current_time, msg=commit_msg))

        remaining -= burst
        current_time += timedelta(minutes=random.randint(10, 40))

    return events

def scenario_e3_interrupted_narrative(num_events=1000):
    """
    Scenario E3: Interrupted Narrative
    One feature unfolds in separated episodes with long unrelated blocks between them.
    """
    events = []
    current_time = datetime(2026, 5, 1, 9, 0, 0)

    target_feature = {
        "name": "Auth Migration",
        "files": ["auth.ts", "jwt.ts", "session.ts"],
        "msgs": ["start migration", "token exchange", "finalize callback"],
        "branch": "feat/auth-migration",
    }
    other_features = [
        {"name": "Payments", "files": ["payments.ts"], "branch": "feat/pay", "msg": "payments work"},
        {"name": "Search", "files": ["search.ts"], "branch": "feat/search", "msg": "search work"},
        {"name": "Infra", "files": ["infra.ts"], "branch": "feat/infra", "msg": "infra work"},
    ]

    target_blocks = [max(1, num_events // 8), max(1, num_events // 8), max(1, num_events // 8)]
    filler_total = max(0, num_events - sum(target_blocks))
    filler_blocks = [filler_total // 2, filler_total - (filler_total // 2)]

    # Target Episode 1
    for _ in range(target_blocks[0]):
        current_time += timedelta(minutes=random.randint(1, 3))
        events.append(
            build_event(
                target_feature["name"],
                random.choice(target_feature["files"]),
                target_feature["branch"],
                current_time,
                msg=target_feature["msgs"][0],
            )
        )

    # Unrelated block 1
    for _ in range(filler_blocks[0]):
        feat = random.choice(other_features)
        current_time += timedelta(minutes=random.randint(1, 3))
        events.append(build_event(feat["name"], random.choice(feat["files"]), feat["branch"], current_time, msg=feat["msg"]))

    current_time += timedelta(minutes=90)

    # Target Episode 2
    for _ in range(target_blocks[1]):
        current_time += timedelta(minutes=random.randint(1, 3))
        events.append(
            build_event(
                target_feature["name"],
                random.choice(target_feature["files"]),
                target_feature["branch"],
                current_time,
                msg=target_feature["msgs"][1],
            )
        )

    # Unrelated block 2
    for _ in range(filler_blocks[1]):
        feat = random.choice(other_features)
        current_time += timedelta(minutes=random.randint(1, 3))
        events.append(build_event(feat["name"], random.choice(feat["files"]), feat["branch"], current_time, msg=feat["msg"]))

    current_time += timedelta(minutes=120)

    # Target Episode 3
    for _ in range(target_blocks[2]):
        current_time += timedelta(minutes=random.randint(1, 3))
        events.append(
            build_event(
                target_feature["name"],
                random.choice(target_feature["files"]),
                target_feature["branch"],
                current_time,
                msg=target_feature["msgs"][2],
            )
        )

    return events[:num_events]

def scenario_e2a_twin_with_ticket_anchors(num_events=1000):
    """
    Scenario E2A: Feature twins with explicit ticket anchors.
    """
    events = []
    shared_files = ["auth.ts", "jwt.ts", "session.ts"]
    branch_pool = ["feat/auth", "feat/auth-v2", "main"]
    features = [
        {"name": "Authentication Refactor", "ticket": "AUTH-123"},
        {"name": "Authentication Migration", "ticket": "AUTH-456"},
    ]

    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active_idx = 0
    while remaining > 0:
        feat = features[active_idx]
        burst = min(remaining, random.randint(10, 25))
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            events.append(
                build_event(
                    feat["name"],
                    random.choice(shared_files),
                    random.choice(branch_pool),
                    current_time,
                    msg="auth update",
                    ticket_id=feat["ticket"],
                )
            )
        remaining -= burst
        active_idx = 1 - active_idx
        current_time += timedelta(minutes=random.randint(10, 40))
    return events

def scenario_e2b_twin_with_branch_episodes(num_events=1000):
    """
    Scenario E2B: Feature twins with distinct branch episodes only.
    """
    events = []
    shared_files = ["auth.ts", "jwt.ts", "session.ts"]
    features = [
        {"name": "Authentication Refactor", "branch": "feature/auth-refactor"},
        {"name": "Authentication Migration", "branch": "feature/auth-migration"},
    ]

    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active_idx = 0
    while remaining > 0:
        feat = features[active_idx]
        burst = min(remaining, random.randint(10, 25))
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            events.append(
                build_event(
                    feat["name"],
                    random.choice(shared_files),
                    feat["branch"],
                    current_time,
                    msg="auth update",
                )
            )
        remaining -= burst
        active_idx = 1 - active_idx
        current_time += timedelta(minutes=random.randint(10, 40))
    return events

def scenario_e2d_realistic_ticket_anchors(
    num_events=1000,
    coverage=0.45,
    mislabel_rate=0.10,
    shared_ticket_rate=0.20,
):
    """
    Scenario E2D: Feature twins with *realistic* ticket anchors.

    E2A gives every event a ticket and maps tickets one-to-one onto features, so
    grouping by ticket_id reproduces the label exactly. Real repositories do not
    look like that. Here the anchor is correlated with the feature but is not a
    relabeling of it:

    - `coverage`: fraction of events that carry a ticket at all. The rest are
      unanchored, which is the common case in practice.
    - `shared_ticket_rate`: fraction that carry an epic-style ticket touched by
      both features, so the mapping is many-to-many rather than bijective.
    - `mislabel_rate`: fraction that carry the *other* feature's ticket, which is
      what happens when a developer copies a branch name or amends the wrong
      commit.

    A method that genuinely reconstructs features should degrade gracefully here
    rather than collapsing to the anchor.
    """
    events = []
    shared_files = ["auth.ts", "jwt.ts", "session.ts"]
    branch_pool = ["feat/auth", "feat/auth-v2", "main"]
    features = [
        {"name": "Authentication Refactor", "ticket": "AUTH-123"},
        {"name": "Authentication Migration", "ticket": "AUTH-456"},
    ]
    epic_ticket = "AUTH-100"

    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active_idx = 0
    while remaining > 0:
        feat = features[active_idx]
        other = features[1 - active_idx]
        burst = min(remaining, random.randint(10, 25))
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))

            ticket = ""
            if random.random() < coverage:
                roll = random.random()
                if roll < shared_ticket_rate:
                    ticket = epic_ticket
                elif roll < shared_ticket_rate + mislabel_rate:
                    ticket = other["ticket"]
                else:
                    ticket = feat["ticket"]

            events.append(
                build_event(
                    feat["name"],
                    random.choice(shared_files),
                    random.choice(branch_pool),
                    current_time,
                    msg="auth update",
                    ticket_id=ticket,
                )
            )
        remaining -= burst
        active_idx = 1 - active_idx
        current_time += timedelta(minutes=random.randint(10, 40))
    return events


def scenario_e2c_twin_with_pr_anchors(num_events=1000):
    """
    Scenario E2C: Feature twins with PR anchors only.
    """
    events = []
    shared_files = ["auth.ts", "jwt.ts", "session.ts"]
    branch_pool = ["feat/auth", "feat/auth-v2", "main"]
    features = [
        {"name": "Authentication Refactor", "pr": "PR-88"},
        {"name": "Authentication Migration", "pr": "PR-91"},
    ]

    current_time = datetime(2026, 5, 1, 9, 0, 0)
    remaining = num_events
    active_idx = 0
    while remaining > 0:
        feat = features[active_idx]
        burst = min(remaining, random.randint(10, 25))
        for _ in range(burst):
            current_time += timedelta(minutes=random.randint(1, 3))
            events.append(
                build_event(
                    feat["name"],
                    random.choice(shared_files),
                    random.choice(branch_pool),
                    current_time,
                    msg="auth update",
                    pr_id=feat["pr"],
                )
            )
        remaining -= burst
        active_idx = 1 - active_idx
        current_time += timedelta(minutes=random.randint(10, 40))
    return events
