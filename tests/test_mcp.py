"""Serveur MCP d'Aria (app/mcp.py) : protocole stdio (JSON-RPC, une ligne par message), outils, erreurs. L'API d'Aria
est simulée : rien n'est lancé."""

import io
import json
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

from app import mcp

OPTIONS = {"formats": ["1:1 (Square)", "16:9 (Widescreen)"],
           "workflows": {"texte_vers_image": {"defaut": "krea2_turbo", "choix": [
               {"id": "krea2_turbo", "nom": "Krea 2 Turbo", "loras": []},
               {"id": "qwen_image_2_1", "nom": "Qwen-Image 2.1", "loras": []}]}}}
ETAT = {"comfyui": {"etat": "veille"}, "llama": {"etat": "actif", "modele": "Bonsai 2 27B"},
        "vram_utilisee_go": 3.1, "vram_totale_go": 16.0, "commande": None}


def echanger(*messages) -> list[dict]:
    """Les réponses du serveur à ces messages, passés comme Claude les envoie : une ligne JSON chacun."""
    entree = io.StringIO("".join((m if isinstance(m, str) else json.dumps(m)) + "\n" for m in messages))
    sortie = io.StringIO()
    mcp.servir(entree, sortie)
    lignes = sortie.getvalue().splitlines()
    return [json.loads(ligne) for ligne in lignes]


def appel(nom, arguments=None, ident=1):
    return {"jsonrpc": "2.0", "id": ident, "method": "tools/call", "params": {"name": nom, "arguments": arguments or {}}}


class FausseAria:
    """Remplace mcp.appeler : rend une réponse par route et garde les demandes."""

    def __init__(self, **reponses):
        self.reponses, self.demandes = reponses, []

    def __call__(self, methode, chemin, corps=None):
        self.demandes.append((methode, chemin, corps))
        reponse = self.reponses[chemin]
        if isinstance(reponse, Exception):
            raise reponse
        return reponse


class Protocole(unittest.TestCase):
    def test_poignee_de_main(self):
        r = echanger({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                      "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "c"}}},
                     {"jsonrpc": "2.0", "method": "notifications/initialized"},  # notification : pas de réponse
                     {"jsonrpc": "2.0", "id": 2, "method": "ping"})
        self.assertEqual(len(r), 2)
        self.assertEqual(r[0]["result"]["protocolVersion"], "2025-06-18")  # une version connue est reprise
        self.assertEqual(r[0]["result"]["capabilities"], {"tools": {"listChanged": False}})
        self.assertEqual(r[0]["result"]["serverInfo"]["name"], "aria-studio")
        self.assertEqual(r[1], {"jsonrpc": "2.0", "id": 2, "result": {}})
        inconnue = echanger({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2099-01-01"}})
        self.assertEqual(inconnue[0]["result"]["protocolVersion"], mcp.PROTOCOLE)  # sinon, la sienne

    def test_outils_listes(self):
        outils = echanger({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})[0]["result"]["tools"]
        self.assertEqual([o["name"] for o in outils],
                         ["etat", "ecrire_prompt", "generer_image", "suivre_rendu", "fiche_image"])
        for o in outils:
            self.assertEqual(o["inputSchema"]["type"], "object", o["name"])
            self.assertTrue(o["description"], o["name"])

    def test_erreurs_du_protocole(self):
        r = echanger("pas du json", [1, 2], {"jsonrpc": "2.0", "id": 3, "method": "resources/list"}, appel("supprimer", ident=4))
        self.assertEqual([x["error"]["code"] for x in r], [-32700, -32600, -32601, -32602])
        self.assertIn("Outil inconnu : supprimer", r[3]["error"]["message"])

    def test_la_sortie_n_a_que_des_messages(self):
        sortie = io.StringIO()
        mcp.servir(io.StringIO('{"jsonrpc":"2.0","id":1,"method":"ping"}\n\n{"jsonrpc":"2.0","id":2,"method":"ping"}\n'),
                   sortie)
        self.assertEqual(sortie.getvalue(), '{"jsonrpc": "2.0", "id": 1, "result": {}}\n{"jsonrpc": "2.0", "id": 2, "result": {}}\n')

    def test_une_panne_n_arrete_pas_le_serveur(self):
        with mock.patch.dict(mcp.ACTIONS, {"etat": mock.Mock(side_effect=RuntimeError("boum"))}), \
                mock.patch.object(mcp.sys, "stderr", io.StringIO()):
            r = echanger(appel("etat"), {"jsonrpc": "2.0", "id": 2, "method": "ping"})
        self.assertEqual(r[0]["error"]["code"], -32603)
        self.assertEqual(r[1]["result"], {})


class Outils(unittest.TestCase):
    def resultat(self, aria, nom, arguments=None):
        with mock.patch.object(mcp, "appeler", aria):
            r = echanger(appel(nom, arguments))[0]["result"]
        texte = r["content"][0]["text"]
        return r["isError"], (texte if r["isError"] else json.loads(texte))

    def test_etat(self):
        erreur, e = self.resultat(FausseAria(**{"/api/etat": ETAT, "/api/plateau/options": OPTIONS}), "etat")
        self.assertFalse(erreur)
        self.assertEqual(e["modeles_image"], [{"id": "krea2_turbo", "nom": "Krea 2 Turbo"},
                                              {"id": "qwen_image_2_1", "nom": "Qwen-Image 2.1"}])
        self.assertEqual((e["modele_par_defaut"], e["formats"][0]), ("krea2_turbo", "1:1 (Square)"))
        self.assertEqual(e["moteurs"]["llm_charge"], "Bonsai 2 27B")

    def test_ecrire_puis_generer_puis_suivre(self):
        aria = FausseAria(**{
            "/api/prompt/optimiser": {"prompt": "A ginger cat asleep on a windowsill.", "mots": 7, "modele": "bonsai.gguf"},
            "/api/creations": {"id": "c0ffee123456", "etat": "attente", "workflow": "krea2_turbo", "format": "1:1 (Square)",
                               "nombre": 2},
            "/api/commandes/c0ffee123456": {"id": "c0ffee123456", "etat": "termine", "etape": "", "progression": 1,
                                            "images": ["20261119_a", "20261119_b"], "erreur": None}})
        _, p = self.resultat(aria, "ecrire_prompt", {"idee": "un chat roux endormi", "modele": "krea2_turbo"})
        self.assertEqual(p["prompt"], "A ginger cat asleep on a windowsill.")
        self.assertEqual(aria.demandes[0][2], {"atelier": "image", "idee": "un chat roux endormi", "workflow": "krea2_turbo"})
        _, c = self.resultat(aria, "generer_image", {"prompt": p["prompt"], "idee": "un chat roux endormi", "nombre": 2})
        self.assertEqual(c["commande"], "c0ffee123456")
        self.assertEqual(aria.demandes[1][2], {"prompt": p["prompt"], "idee": "un chat roux endormi", "workflow": None,
                                               "seed": None, "nombre": 2})  # le format de l'utilisateur s'il n'en donne pas
        _, s = self.resultat(aria, "suivre_rendu", {"commande": "c0ffee123456"})
        self.assertEqual(s["etat"], "termine")
        self.assertEqual(s["images"][0], {"id": "20261119_a", "chemin": str(mcp.CREATIONS / "20261119_a.png")})

    def test_l_erreur_d_aria_rendue_a_claude(self):
        """Le filtre de protection des mineurs, comme toute erreur de l'API, revient tel quel, en erreur d'outil."""
        aria = FausseAria(**{"/api/creations": mcp.ErreurOutil("Refusé : un mineur dans une scène sexuelle.")})
        erreur, texte = self.resultat(aria, "generer_image", {"prompt": "x"})
        self.assertTrue(erreur)
        self.assertEqual(texte, "Refusé : un mineur dans une scène sexuelle.")

    def test_parametres_controles(self):
        aria = FausseAria()
        for nom, arguments, attendu in (("generer_image", {}, "Paramètre manquant : prompt"),
                                        ("generer_image", {"prompt": "x", "nombre": 9}, "de 1 à 4"),
                                        ("etat", {"supprimer": True}, "Paramètre inconnu : supprimer"),
                                        ("fiche_image", {"image": "../../reglages"}, "Identifiant invalide"),
                                        ("suivre_rendu", {"commande": "a/b"}, "Identifiant invalide")):
            erreur, texte = self.resultat(aria, nom, arguments)
            self.assertTrue(erreur, nom)
            self.assertIn(attendu, texte)
        self.assertEqual(aria.demandes, [])  # rien n'est parti vers Aria


class Api(unittest.TestCase):
    """mcp.appeler : le port et le jeton du lancement en cours (cache/session.json), l'erreur d'Aria telle quelle."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.session = Path(self.tmp.name) / "session.json"
        p = mock.patch.object(mcp, "SESSION", self.session)
        p.start()
        self.addCleanup(p.stop)

    def test_aria_pas_lancee(self):
        with self.assertRaisesRegex(mcp.ErreurOutil, "n'est pas lancée"):
            mcp.appeler("GET", "/api/etat")
        self.session.write_text('{"port": 8075, "jeton": "j"}', encoding="utf-8")
        with mock.patch.object(mcp.urllib.request, "urlopen", side_effect=urllib.error.URLError("refusé")):
            with self.assertRaisesRegex(mcp.ErreurOutil, "n'est pas lancée"):  # session restée après un arrêt brutal
                mcp.appeler("GET", "/api/etat")

    def test_jeton_et_adresse(self):
        self.session.write_text('{"port": 8075, "jeton": "le-jeton", "pid": 1}', encoding="utf-8")
        reponse = mock.MagicMock()
        reponse.__enter__.return_value.read.return_value = b'{"ok": true}'
        with mock.patch.object(mcp.urllib.request, "urlopen", return_value=reponse) as urlopen:
            self.assertEqual(mcp.appeler("POST", "/api/creations", {"prompt": "x"}), {"ok": True})
        requete = urlopen.call_args[0][0]
        self.assertEqual(requete.full_url, "http://127.0.0.1:8075/api/creations")
        self.assertEqual(requete.get_header("X-aria-jeton"), "le-jeton")
        self.assertEqual(json.loads(requete.data), {"prompt": "x"})

    def test_detail_de_l_erreur(self):
        self.session.write_text('{"port": 8075, "jeton": "j"}', encoding="utf-8")
        refus = urllib.error.HTTPError("u", 422, "x", {}, io.BytesIO('{"detail": "Le prompt est vide"}'.encode()))
        with mock.patch.object(mcp.urllib.request, "urlopen", side_effect=refus):
            with self.assertRaisesRegex(mcp.ErreurOutil, "^Le prompt est vide$"):
                mcp.appeler("POST", "/api/creations", {})


if __name__ == "__main__":
    unittest.main()


class CarteReglages(unittest.TestCase):
    def test_commande_et_json(self):
        from app import main
        with mock.patch.object(main.sys, "executable", r"C:\Aria\programme\python\pythonw.exe"), \
                mock.patch.object(main, "ROOT", Path(r"C:\Aria\programme")):
            m = main.commande_mcp()
        python, script = r"C:\Aria\programme\python\python.exe", str(Path(r"C:\Aria\programme") / "app" / "mcp.py")
        self.assertEqual(m["python"], python)  # pythonw n'a ni entrée ni sortie standard
        self.assertEqual(m["commande"],
                         f'claude mcp add --transport stdio --scope user aria-studio -- "{python}" "{script}"')
        self.assertEqual(m["serveur"], {"type": "stdio", "command": python, "args": [script]})
        self.assertTrue((Path(main.ROOT) / "app" / "mcp.py").is_file())  # le script visé existe
