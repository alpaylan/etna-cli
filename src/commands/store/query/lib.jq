
def experiment_by_id (id): .experiments[] | select(.id == id);
def experiments_by_name (name): .experiments[] | select(.name == name);

# Get the last experiment by name by looking up the experiment times 
# from the snapshot list and then filtering the experiments by name
# and adding the time to the experiment object
def last_experiment_by_name (name):
    # Step 0: Save the root object to use it later 
    . as $root
    # Step 1: Filter snapshots to find the ones with experiments
    | .snapshots
    | map(select(.typ.experiment != null))                          # Filter only snapshots with experiments
    | map({id: .hash, time: .typ.experiment.time}) as $times        # Create a list of hashes and times

    # Step 2: Match experiments by name and enrich them with the time data
    | $root 
    | .experiments
    | map(select(.name == name))                                    # Select experiment by name
    | map(.time = ($times | map(select(.id == .id)) | .[0].time))   # Add time to the experiment
    | sort_by(.time)                                                # Sort by time
    | last                                                          # Get the last one
    ;

def metrics_by_experiment_id (id): .metrics[] | select(.experiment_id == id);

def metrics_by_json_object (json): 
    .metrics
    | map(select(.data | contains(json)));

def metrics_by_json_string (json_string): 
    metrics_by_json_object(json_string | fromjson);

def snapshots_by_json_object (json): 
    .snapshots
    | map(select(.typ | contains(json)));

def snapshots_by_json_string (json_string):
    snapshots_by_json_object(json_string | fromjson);

def snapshots_by_name (name): 
    snapshots_by_json_object({name: name});

def snapshot_by_hash (hash): 
    .snapshots[] | select(.hash == hash);