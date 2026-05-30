import uuid
import random
from datetime import datetime, timedelta

FEATURES = [
    {"name": "Authentication", "files": ["auth.ts", "jwt.ts", "login.ts"], "branch": "feature/auth"},
    {"name": "Payments", "files": ["payment.ts", "invoice.ts"], "branch": "feature/payments"},
    {"name": "SSO Migration", "files": ["sso.ts", "auth.ts", "oauth.ts"], "branch": "feature/sso-mig"},
    {"name": "Memory Leak Hotfix", "files": ["memory.ts", "utils.ts"], "branch": "hotfix/memleak"}
]

def generate_synthetic_events(num_events=1000):
    events = []
    current_time = datetime(2026, 5, 1, 9, 0, 0)
    
    for _ in range(num_events):
        feat = random.choice(FEATURES)
        
        # Simulating time gaps (in minutes)
        # Small chance of context switch / session break
        delta_mins = random.randint(1, 10)
        if random.random() < 0.05:
            delta_mins += random.randint(60, 600)
            
        current_time += timedelta(minutes=delta_mins)
        
        # Add realistic noise/variance
        # 10% chance developer committed straight to main or branch tracking was lost
        branch = feat["branch"] if random.random() < 0.9 else "main"
        
        # 20% chance they wrote a commit message associated with this event
        commit_msg = f"Work on {feat['name']}" if random.random() < 0.2 else ""
        
        e = {
            "event_id": str(uuid.uuid4()),
            "timestamp": current_time.timestamp(),
            "file_path": random.choice(feat["files"]),
            "branch": branch,
            "commit_msg": commit_msg,
            "ground_truth": feat["name"] # This is what we want the algorithm to uncover
        }
        events.append(e)
        
    return events

if __name__ == '__main__':
    import json
    events = generate_synthetic_events(10)
    print(json.dumps(events, indent=2))
