"""Pour les tests qui ne portent pas sur la validation des steps : chaque checkpoint présent a des steps connus (ceux
validés s'il y en a, sinon le défaut de la fiche de son workflow). Sans cela, l'origine de H3 (8 par défaut, 12
recommandé) et celle de Qwen-Image 2.1 (25, 40) seraient « à valider » et refusées partout.

Dans un module de test :  setUpModule, tearDownModule = steps_connus.module()
"""

from unittest import mock

from app import workflows

_VRAI = workflows.Workflow.steps_de


def _steps_de(wf, fichier):
    return _VRAI(wf, fichier) or int(wf.fiche.get("defauts", {}).get("steps", 0))


def module():
    patch = mock.patch.object(workflows.Workflow, "steps_de", _steps_de)
    return patch.start, patch.stop
