import itertools
from collections import defaultdict
from synthetic_data import generate_synthetic_events

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
