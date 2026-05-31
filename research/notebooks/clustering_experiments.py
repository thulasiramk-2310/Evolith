import sys
import os
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

import networkx as nx
from networkx.algorithms.community import louvain_communities
from sklearn.cluster import DBSCAN
import numpy as np
import itertools
from collections import defaultdict

from synthetic_data import generate_synthetic_events
from feature_reconstruction_simulator import (
    assign_episode_ids,
    calculate_similarity,
    calculate_similarity_hybrid,
    calculate_similarity_weighted,
    compute_file_idf,
    evaluate_clusters,
)

def build_baseline_clusters(events, threshold=0.39):
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
            
    for e1, e2 in itertools.combinations(events, 2):
        if calculate_similarity(e1, e2) >= threshold:
            union(e1['event_id'], e2['event_id'])
            
    clusters = defaultdict(list)
    for e in events:
        clusters[find(e['event_id'])].append(e)
        
    return sorted(list(clusters.values()), key=len, reverse=True)

def build_dbscan_clusters(events, eps=0.6, min_samples=3):
    n = len(events)
    if n == 0: return []
    dist_matrix = np.zeros((n, n))
    for i in range(n):
        for j in range(i+1, n):
            sim = calculate_similarity(events[i], events[j])
            dist = max(0.0, 1.0 - sim)
            dist_matrix[i, j] = dist
            dist_matrix[j, i] = dist
            
    db = DBSCAN(eps=eps, min_samples=min_samples, metric='precomputed')
    labels = db.fit_predict(dist_matrix)
    
    clusters = defaultdict(list)
    for i, label in enumerate(labels):
        if label != -1:
            clusters[label].append(events[i])
            
    return sorted(list(clusters.values()), key=len, reverse=True)

def build_louvain_clusters(
    events,
    threshold=0.1,
    use_file_rarity_weighting=True,
    use_episode_continuity=True,
    use_hub_penalty=True,
):
    G = nx.Graph()
    for e in events:
        G.add_node(e['event_id'], data=e)

    idf_map, min_idf, max_idf = compute_file_idf(events)
    episode_map = assign_episode_ids(events) if use_episode_continuity else None

    pending_edges = []
    for e1, e2 in itertools.combinations(events, 2):
        if use_file_rarity_weighting and use_episode_continuity:
            sim = calculate_similarity_hybrid(
                e1, e2, idf_map, min_idf, max_idf, episode_map=episode_map
            )
        elif use_file_rarity_weighting:
            sim = calculate_similarity_weighted(e1, e2, idf_map, min_idf, max_idf)
        else:
            sim = calculate_similarity(e1, e2)
        if sim > threshold:
            pending_edges.append((e1['event_id'], e2['event_id'], sim))

    for u, v, w in pending_edges:
        G.add_edge(u, v, weight=w)

    if use_hub_penalty and G.number_of_edges() > 0:
        for u, v, data in G.edges(data=True):
            deg_u = max(1, G.degree(u))
            deg_v = max(1, G.degree(v))
            data["weight"] = data["weight"] / np.sqrt(deg_u * deg_v)
            
    # Louvain
    communities = louvain_communities(G, weight='weight')
    
    event_dict = {e['event_id']: e for e in events}
    clusters = []
    for comm in communities:
        cluster = [event_dict[node] for node in comm]
        clusters.append(cluster)
        
    return sorted(clusters, key=len, reverse=True)

if __name__ == '__main__':
    np.random.seed(42)
    # Using fixed events for comparison across algorithms
    events = generate_synthetic_events(1000)
    
    print("================== BASELINE ==================")
    clusters_baseline = build_baseline_clusters(events, threshold=0.39)
    evaluate_clusters(clusters_baseline, len(events))
    print("\n")
    
    print("================== DBSCAN ==================")
    clusters_dbscan = build_dbscan_clusters(events, eps=0.65, min_samples=3)
    evaluate_clusters(clusters_dbscan, len(events))
    print("\n")
    
    print("================== LOUVAIN ==================")
    clusters_louvain = build_louvain_clusters(events, threshold=0.2)
    evaluate_clusters(clusters_louvain, len(events))
