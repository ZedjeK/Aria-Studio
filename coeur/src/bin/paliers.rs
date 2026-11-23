//! Banc de palier (outil du mainteneur), comme `python -m app.paliers` en v2. Aria doit être quittée : le banc lance
//! lui-même ComfyUI et llama-server sur leurs ports habituels.
//!
//! ```text
//! cargo run -p aria-coeur --bin paliers --release [ids de workflows ou de GGUF…]   (sans argument : tout le catalogue)
//! cargo run -p aria-coeur --bin paliers --release -- --ecrire [dossier]            (reporte un banc dans les fiches)
//! ```
//!
//! `--ecrire` se lance depuis la racine du dépôt : il réécrit `workflows/<id>/fiche.yaml` et `catalogue/llm.yaml`
//! d'après `data\bancs\paliers\<dossier>\resultats.json` (le dernier banc si aucun dossier n'est donné).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use aria_coeur::comfy::{self, DOSSIER_TRAVAIL};
use aria_coeur::comfy_serveur::ServeurComfy;
use aria_coeur::emplacements::Emplacements;
use aria_coeur::llama_serveur::ServeurLlama;
use aria_coeur::llm::Lieux;
use aria_coeur::{Dossier, catalogue, paliers, reglages};

/// Les ports d'Aria (comme `appli/src/moteurs.rs`) : le banc refuse de partir si Aria les tient.
const PORT_COMFYUI: u16 = 8195;
const PORT_LLAMA: u16 = 8095;

fn emplacements() -> Result<Emplacements, String> {
    let profil = Dossier::du_profil().map_err(|e| e.to_string())?;
    let dossier_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| profil.racine().to_path_buf());
    Ok(Emplacements::decider(&dossier_exe, &profil))
}

/// Le ComfyUI d'Aria : celui de « J'ai déjà ComfyUI », sinon le portable des moteurs.
fn comfyui(e: &Emplacements) -> Result<PathBuf, String> {
    let externe = reglages::section(&e.dossier, "emplacements")
        .and_then(|s| s.get("comfyui")?.as_str().map(str::trim).map(str::to_owned))
        .filter(|c| !c.is_empty());
    if let Some(chemin) = externe {
        return Ok(PathBuf::from(chemin));
    }
    let catalogue = catalogue::Catalogue::embarque()?;
    let sous_dossier = catalogue
        .moteur("comfyui")
        .map_or("comfyui", |m| m.dossier.as_str());
    Ok(e.moteurs.join(sous_dossier))
}

fn mesurer(ids: &[String]) -> Result<(), String> {
    let e = emplacements()?;
    let cache = e.dossier.cache();
    let mut comfy = ServeurComfy::new(
        comfy::Lancement {
            dossier: comfyui(&e)?,
            port: PORT_COMFYUI,
            chemins_modeles: cache.join("comfyui_chemins.yaml"),
            travail: cache.join(DOSSIER_TRAVAIL),
        },
        e.data.clone(),
    );
    let mut llama = ServeurLlama::new(PORT_LLAMA, e.data.clone());
    let lieux = Lieux {
        dossier: e.dossier.clone(),
        modeles: e.modeles.clone(),
        moteurs: e.moteurs.clone(),
        port: PORT_LLAMA,
    };
    let mut journal = |ligne: &str| println!("{}", paliers::horodater(ligne));
    let resultats = paliers::banc(&mut comfy, &mut llama, &lieux, &e.data, ids, &mut journal)?;
    println!(
        "Banc écrit dans {} ({} workflow(s), {} LLM)",
        paliers::dossier_bancs(&e.data).display(),
        resultats.workflows.len(),
        resultats.llm.len()
    );
    Ok(())
}

fn ecrire(nom: Option<&str>) -> Result<(), String> {
    let e = emplacements()?;
    let dossier = match nom {
        Some(n) => paliers::dossier_bancs(&e.data).join(n),
        None => paliers::dernier_banc(&e.data).ok_or("Aucun banc de palier écrit")?,
    };
    let racine = std::env::current_dir().map_err(|e| e.to_string())?;
    paliers::ecrire(&racine, &paliers::lire_banc(&dossier)?)?;
    println!(
        "Fiches mises à jour depuis {}",
        dossier
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let resultat = if args.iter().any(|a| a == "--ecrire") {
        let reste: Vec<&String> = args.iter().filter(|a| *a != "--ecrire").collect();
        ecrire(reste.first().map(|s| s.as_str()))
    } else {
        mesurer(&args)
    };
    match resultat {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
