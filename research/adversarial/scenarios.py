import uuid
import random
from datetime import datetime, timedelta

def build_event(feat_name, fpath, branch, ctime, msg=""):
    return {
        "event_id": str(uuid.uuid4()),
        "timestamp": ctime.timestamp(),
        "file_path": fpath,
        "branch": branch,
        "commit_msg": msg,
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
