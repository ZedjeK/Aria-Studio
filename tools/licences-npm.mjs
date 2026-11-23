// Les paquets npm dont le code ou les fichiers finissent dans l'interface livrée, pour LICENCES-TIERS.txt.
//
//     node tools/licences-npm.mjs <dossier d'une construction avec sourcemaps> <dossier interface> [fichier de sortie]
//
// Le code embarqué se lit dans les sourcemaps d'une construction de l'interface faite à part (tools/licences-tierces.ps1
// la lance dans un dossier temporaire) ; les polices, copiées par Vite sans sourcemap, viennent des paquets @fontsource
// déclarés. Pour chaque paquet : une ligne « nom|version|licence|dépôt », puis ses textes de licence, chacun encadré par
// « @@texte <fichier> » et « @@fin », écrits en UTF-8 dans le fichier de sortie (sinon sur la sortie standard). Lu par
// tools/licences-tierces.ps1.

import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [sortie, dossierInterface, fichierSortie] = process.argv.slice(2);
if (!sortie || !dossierInterface) {
  console.error('Usage : node tools/licences-npm.mjs <sortie de construction> <dossier interface>');
  process.exit(2);
}

const paquets = new Set();
const assets = join(sortie, 'assets');
for (const fichier of readdirSync(assets).filter((f) => f.endsWith('.map'))) {
  const carte = JSON.parse(readFileSync(join(assets, fichier), 'utf8'));
  for (const source of carte.sources) {
    const i = source.lastIndexOf('node_modules/');
    if (i < 0) continue;
    const morceaux = source.slice(i + 'node_modules/'.length).split('/');
    paquets.add(morceaux[0].startsWith('@') ? `${morceaux[0]}/${morceaux[1]}` : morceaux[0]);
  }
}
const declares = JSON.parse(readFileSync(join(dossierInterface, 'package.json'), 'utf8')).dependencies ?? {};
for (const nom of Object.keys(declares)) {
  if (nom.startsWith('@fontsource/')) paquets.add(nom);
}

/** Le dépôt d'un paquet, en adresse complète. */
function depot(paquet) {
  const brut = typeof paquet.repository === 'string' ? paquet.repository : (paquet.repository?.url ?? '');
  const propre = brut.replace(/^git\+/, '').replace(/\.git$/, '');
  return /^[\w.-]+\/[\w.-]+$/.test(propre) ? `https://github.com/${propre}` : propre;
}

const motif = /^(licen[cs]e|copying|notice|ofl)/i;
const lignes = [];
for (const nom of [...paquets].sort()) {
  const dossier = join(dossierInterface, 'node_modules', ...nom.split('/'));
  const paquet = JSON.parse(readFileSync(join(dossier, 'package.json'), 'utf8'));
  lignes.push([nom, paquet.version, paquet.license ?? '', depot(paquet)].join('|'));
  for (const fichier of readdirSync(dossier).filter((f) => motif.test(f)).sort()) {
    const texte = readFileSync(join(dossier, fichier), 'utf8').replace(/\r\n/g, '\n').trim();
    lignes.push(`@@texte ${fichier}`, texte, '@@fin');
  }
}
const texte = `${lignes.join('\n')}\n`;
if (fichierSortie) writeFileSync(fichierSortie, texte, 'utf8');
else process.stdout.write(texte);
