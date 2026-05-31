import itertools
from collections import defaultdict
import math
from synthetic_data import generate_synthetic_events

def assign_episode_ids(events, max_gap_seconds=20 * 60, max_episode_len=30):
    """
    Infer episode continuity using temporal proximity plus light branch continuity.
    Returns a map: event_id -> episode_id.
    """
    if not events:
        return {}

    ordered = sorted(events, key=lambda e: e["timestamp"])
    episode_map = {}
    episode_id = 0
    episode_len = 0
    prev = None

    for event in ordered:
        start_new = False
        if prev is None:
            start_new = True
        else:
            gap = abs(event["timestamp"] - prev["timestamp"])
            branch_changed = event["branch"] != prev["branch"]
            non_main_shift = branch_changed and event["branch"] != "main" and prev["branch"] != "main"
            if gap > max_gap_seconds or non_main_shift or episode_len >= max_episode_len:
                start_new = True

        if start_new:
            episode_id += 1
            episode_len = 0

        episode_len += 1
        episode_map[event["event_id"]] = episode_id
        prev = event

    return episode_map

def compute_file_idf(events):
    total = max(1, len(events))
    file_counts = defaultdict(int)
    for event in events:
        file_counts[event["file_path"]] += 1

    idf = {}
    for file_path, count in file_counts.items():
        # Smoothed IDF to avoid division edge cases and keep values stable.
        idf[file_path] = math.log((1 + total) / (1 + count)) + 1.0

    if not idf:
        return {}, 1.0, 1.0

    min_idf = min(idf.values())
    max_idf = max(idf.values())
    return idf, min_idf, max_idf

def _normalized_idf(file_path, idf_map, min_idf, max_idf):
    if file_path not in idf_map:
        return 1.0
    if max_idf <= min_idf:
        return 1.0
    return (idf_map[file_path] - min_idf) / (max_idf - min_idf)

def calculate_similarity(e1, e2):
    score = 0.0
    
    # 1. Branch similarity
    if e1['branch'] != 'main' and e1['branch'] == e2['branch']:
        score += 0.40
        
    # 2. Path similarity 
    if e1['file_path'] == e2['file_path']:
        score += 0.30
        
    # 3. Temporal similarity (scales from 0.0 to 0.2 within a 30-minute window)
    time_diff = abs(e1['timestamp'] - e2['timestamp'])
    max_temporal_window = 1800 # 30 mins
    if time_diff < max_temporal_window:
        score += 0.20 * (1.0 - (time_diff / max_temporal_window))
        
    # 4. Commit similarity (e.g. correlated by issue # in future, here just string match)
    if e1['commit_msg'] and e1['commit_msg'] == e2['commit_msg']:
        score += 0.10
        
    return score

def calculate_similarity_weighted(e1, e2, idf_map, min_idf, max_idf):
    score = 0.0

    # 1. Branch similarity
    if e1['branch'] != 'main' and e1['branch'] == e2['branch']:
        score += 0.40

    # 2. Path similarity with rarity weighting
    if e1['file_path'] == e2['file_path']:
        rarity = _normalized_idf(e1['file_path'], idf_map, min_idf, max_idf)
        # Keep a small floor so shared common files are not zeroed out completely.
        rarity = max(0.10, rarity)
        score += 0.30 * rarity

    # 3. Temporal similarity
    time_diff = abs(e1['timestamp'] - e2['timestamp'])
    max_temporal_window = 1800
    if time_diff < max_temporal_window:
        score += 0.20 * (1.0 - (time_diff / max_temporal_window))

    # 4. Commit similarity
    if e1['commit_msg'] and e1['commit_msg'] == e2['commit_msg']:
        score += 0.10

    return score

def calculate_similarity_hybrid(
    e1,
    e2,
    idf_map,
    min_idf,
    max_idf,
    episode_map=None,
    episode_bonus=0.25,
):
    score = calculate_similarity_weighted(e1, e2, idf_map, min_idf, max_idf)

    # 5. Anchor similarity (ticket/PR)
    t1 = e1.get("ticket_id", "")
    t2 = e2.get("ticket_id", "")
    p1 = e1.get("pr_id", "")
    p2 = e2.get("pr_id", "")

    if t1 and t2:
        if t1 == t2:
            score += 0.45
        else:
            score -= 0.35

    if p1 and p2:
        if p1 == p2:
            score += 0.40
        else:
            score -= 0.30

    if episode_map:
        ep1 = episode_map.get(e1["event_id"])
        ep2 = episode_map.get(e2["event_id"])
        if ep1 is not None and ep1 == ep2:
            score += episode_bonus
    return max(0.0, score)

def build_clusters(events, threshold=0.35):
    # Standard O(N^2) associative union-find to map connected components (Clusters/Features)
    parent = {e['event_id']: e['event_id'] for e in events}
    
    def find(i):
        if parent[i] == i: return i
        parent[i] = find(parent[i])
        return parent[i]
        
    def union(i, j):
        root_i = find(i)
        root_j = find(j)
        if root_i != root_j:
            parent[root_i] = root_j
            
    # Compute similarity between every event pair
    for e1, e2 in itertools.combinations(events, 2):
        if calculate_similarity(e1, e2) >= threshold:
            union(e1['event_id'], e2['event_id'])
            
    # Extract clustered groups
    clusters = defaultdict(list)
    for e in events:
        clusters[find(e['event_id'])].append(e)
        
    # Sort descending by size of the feature
    return sorted(list(clusters.values()), key=len, reverse=True)

def evaluate_clusters(clusters, total_events):
    correct = 0
    print("\n--- Feature Reconstruction Simulator Results ---")
    
    for i, cluster in enumerate(clusters):
        truth_counts = {}
        for e in cluster:
            truth_counts[e['ground_truth']] = truth_counts.get(e['ground_truth'], 0) + 1
            
        # The 'predicted' name of this feature is whatever ground-truth dominates it most
        dominant_truth = max(truth_counts.items(), key=lambda x: x[1])[0]
        correct += truth_counts[dominant_truth]
        
        # Omit tiny noisy single-event clusters from readout
        if len(cluster) > 5:
            print(f"Predicted Feature {i+1} (Size: {len(cluster):>3}) -> Dominant Truth: '{dominant_truth:<20}' (Purity: {truth_counts[dominant_truth]}/{len(cluster)})")
            
    accuracy = correct / total_events
    print(f"\nTotal Overall Reconstruction Purity: {accuracy*100:.2f}%")

if __name__ == '__main__':
    NUM_EVENTS = 1000
    print(f"Generating {NUM_EVENTS} synthetic events...")
    events = generate_synthetic_events(NUM_EVENTS)
    
    print("Clustering events via heuristics...")
    clusters = build_clusters(events, threshold=0.39)
    evaluate_clusters(clusters, NUM_EVENTS)
