import os
import pathlib

from benchtool.Tasks import tasks
from benchtool.Racket import Racket
from benchtool.Types import TrialConfig, ReplaceLevel

import logging

def collect(results: str):
    tool = Racket(results=results,log_level=logging.DEBUG, replace_level=ReplaceLevel.SKIP, log_file='myapp.log')

    for workload in tool.all_workloads():
        tool._log(f'Collecting {workload.name}...', logging.INFO)
        if workload.name in ['BST']:
            for variant in tool.all_variants(workload):
                tool._log(f'Collecting {workload.name} {variant.name}...', logging.INFO)
                if variant.name == 'base':
                    continue

                run_trial = None

                for strategy in tool.all_strategies(workload):           
                    tool._log(f'Collecting {workload.name} {variant.name} {strategy.name}...', logging.INFO)
                    print(tool.all_properties(workload))
                    for property in tool.all_properties(workload):
                        tool._log(f'Collecting {workload.name} {variant.name} {strategy.name} {property}...', logging.INFO)
                        if property.split('_')[1] not in tasks[workload.name][variant.name]:
                            tool._log(f'Skipping {workload.name} {variant.name} {strategy.name} {property}...', logging.INFO)
                            continue
                        property = 'test_' + property       
                        
                        file = f'{workload.name},{strategy.name},{variant.name},{property}'
                        experiment_id = os.environ.get("ETNA_EXPERIMENT_ID") or file


                        if not run_trial:
                            run_trial = tool.apply_variant(workload, variant, no_base=True)

                        tool._log(f'Running {workload.name} {variant.name} {strategy.name} {property}...', logging.INFO)
                        cfg = TrialConfig(workload=workload,
                                        strategy=strategy.name,
                                        property=property,
                                        experiment_id=experiment_id,
                                        trials=10,
                                        timeout=60,
                                        short_circuit=True)
                        run_trial(cfg)


if __name__ == '__main__':
    filepath = pathlib.Path(__file__).resolve().parent
    collect(pathlib.Path(filepath, 'results'))
