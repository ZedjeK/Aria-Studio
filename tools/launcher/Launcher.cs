// Lanceur d'Aria - Studio : démarre le serveur sans console (Python portable du dossier, « pythonw -m app »),
// attend qu'il réponde, puis ouvre l'interface dans le navigateur par défaut. Si Aria tourne déjà, il ouvre juste
// l'interface. Compilé par tools/build_portable.py avec le csc.exe fourni par Windows (C# 5 : pas de $"", ni de ?.).
//
// Installation par l'exe : lancé depuis un zip décompressé, il propose « Installer » ou « Lancer sans installer ».
// Installer copie son dossier dans %USERPROFILE%\.Aria-Studio\programme (la version d'avant devient
// programme.precedent), sans la marque « téléchargé depuis Internet », crée le raccourci du Bureau si la case est
// cochée, puis relance depuis programme\. Pas de proposition si : Aria répond déjà, le lanceur est déjà dans
// programme\, un dossier data\ est à côté de lui (mode portable), --portable, ou « installation.proposee: false »
// dans reglages.json (case « Ne plus me proposer », réactivable dans Réglages → Système).
//
// Réservés aux essais (tests/test_lanceur.py, sans fenêtre ni lancement) :
//   --essai-proposer          code 0 si la fenêtre serait proposée, 2 sinon
//   --essai-installer         installe sans fenêtre (code 0 si c'est fait) ; avec --raccourci, crée le raccourci
//                             dans %USERPROFILE%\Desktop (jamais remplacé s'il existe)
//   --essai-ne-plus-proposer  écrit « installation.proposee: false » dans reglages.json
//   --essai-adresse           écrit sur la sortie l'adresse qu'il ouvrirait (avec le jeton de cache\session.json)
//   --essai-mise-a-jour <pid> fait la mise à jour ci-dessous sans fenêtre et sans relancer Aria
//
// Mise à jour intégrée (app/installation_maj.py) : Aria lance le NOUVEAU lanceur, décompressé dans
// cache\mise-a-jour\Aria-Studio-X.Y.Z\, avec « --mise-a-jour <pid du serveur> <ancien lanceur> », puis s'arrête.
// Ce lanceur attend la fin du serveur, installe comme « Installer » (programme.precedent gardé) et relance Aria depuis
// programme\ ; si l'installation échoue, programme\ n'a pas changé et il relance l'ancien lanceur.
//
// Jeton par lancement (app/jeton.py) : le serveur écrit son jeton dans cache\session.json ; le lanceur ouvre
// « /?jeton=… », que le serveur range dans un cookie avant de rediriger vers « / ».
//
// Icône de notification (tools/launcher/Icone.cs) : après avoir ouvert l'interface, le lanceur reste ouvert et tient
// l'icône tant que le serveur répond. --silencieux (démarrage avec Windows) lance le serveur et l'icône sans ouvrir le
// navigateur.
//
// Compilé avec Icone.cs, System.Windows.Forms, System.Drawing et System.Web.Extensions (lecteur JSON de .NET).

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Forms;

static partial class Launcher
{
    const int PortParDefaut = 8075;
    const int DelaiDemarrageSecondes = 90;
    const string Titre = "Aria Studio";
    const string NomRaccourci = "Aria Studio.lnk";

    static bool essai;  // pas de fenêtre : les erreurs sortent par le code de retour

    [STAThread]
    static int Main(string[] args)
    {
        string racine = Dossier(AppDomain.CurrentDomain.BaseDirectory);
        essai = A(args, "--essai-proposer") || A(args, "--essai-installer") || A(args, "--essai-ne-plus-proposer")
                || A(args, "--essai-adresse") || A(args, "--essai-mise-a-jour") || A(args, "--essai-demarrage")
                || A(args, "--essai-icone");
        bool portable = A(args, "--portable");

        if (A(args, "--essai-proposer")) return DoitProposer(racine, portable) ? 0 : 2;
        if (A(args, "--essai-ne-plus-proposer")) return NePlusProposer() ? 0 : 1;
        if (A(args, "--essai-installer")) return Installer(racine, A(args, "--raccourci")) != null ? 0 : 1;
        if (A(args, "--mise-a-jour") || A(args, "--essai-mise-a-jour")) return MettreAJour(racine, args);
        if (A(args, "--essai-demarrage")) return EssaiDemarrage(racine, args);
        if (A(args, "--essai-icone")) return EssaiIcone(racine, args);

        string url = "http://127.0.0.1:" + LirePort(racine) + "/";
        if (A(args, "--essai-adresse"))
        {
            Console.Out.Write(Adresse(racine, url, 1));
            return 0;
        }
        if (!Repond(url) && DoitProposer(racine, portable))
        {
            Application.EnableVisualStyles();
            bool raccourci, nePlus;
            DialogResult choix = Proposer(out raccourci, out nePlus);
            if (nePlus) NePlusProposer();
            if (choix == DialogResult.Yes)
            {
                string exe = Installer(racine, raccourci);
                if (exe == null) return 1;  // le message est déjà affiché, rien n'a changé dans programme\
                Process.Start(new ProcessStartInfo(exe) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(exe) });
                return 0;
            }
            if (choix != DialogResult.No) return 0;  // fenêtre fermée : on ne lance rien
        }
        return Lancer(racine, url, A(args, "--silencieux"));
    }

    static bool A(string[] args, string option)
    {
        return Array.IndexOf(args, option) >= 0;
    }

    // --- emplacements (même règle qu'app/emplacements.py : USERPROFILE, comme Path.home() de Python) ---

    static string Maison()
    {
        string profil = Environment.GetEnvironmentVariable("USERPROFILE");
        if (string.IsNullOrEmpty(profil)) profil = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        return Path.Combine(profil, ".Aria-Studio");
    }

    static string Programme() { return Path.Combine(Maison(), "programme"); }

    static string Dossier(string chemin)
    {
        return Path.GetFullPath(chemin).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
    }

    static bool MemeDossier(string a, string b)
    {
        return string.Equals(Dossier(a), Dossier(b), StringComparison.OrdinalIgnoreCase);
    }

    static string NomExe()
    {
        return Path.GetFileName(Assembly.GetEntryAssembly().Location);
    }

    // --- la proposition ---

    static bool DoitProposer(string racine, bool portable)
    {
        if (portable) return false;
        if (MemeDossier(racine, Programme())) return false;  // déjà installé
        if (MemeDossier(racine, Programme() + ".precedent")) return false;  // retour à la version d'avant
        if (MemeDossier(racine, Maison())) return false;  // ancienne disposition (tools/build_portable.py la range)
        if (Directory.Exists(Path.Combine(racine, "data"))) return false;  // mode portable
        return !NeProposePlus();
    }

    // Réglages de l'utilisateur : reglages.json (app/fichier_reglages.py), section « installation », clé « proposee ».
    static string FichierReglages() { return Path.Combine(Maison(), "reglages.json"); }
    static string AncienFichierChoix() { return Path.Combine(Maison(), "aria.yaml"); }

    // Le contenu de reglages.json ; null s'il est absent ou illisible (on ne l'écrase jamais alors).
    static Dictionary<string, object> LireReglages(out bool illisible)
    {
        illisible = false;
        string fichier = FichierReglages();
        if (!File.Exists(fichier)) return null;
        try
        {
            var d = new JavaScriptSerializer().DeserializeObject(File.ReadAllText(fichier)) as Dictionary<string, object>;
            illisible = d == null;
            return d;
        }
        catch (ArgumentException) { illisible = true; }
        catch (InvalidOperationException) { illisible = true; }
        catch (IOException) { illisible = true; }
        catch (UnauthorizedAccessException) { illisible = true; }
        return null;
    }

    static bool NeProposePlus()
    {
        bool illisible;
        Dictionary<string, object> reglages = LireReglages(out illisible);
        if (reglages != null)
        {
            object section, valeur;
            var installation = reglages.TryGetValue("installation", out section) ? section as Dictionary<string, object> : null;
            if (installation != null && installation.TryGetValue("proposee", out valeur) && valeur is bool)
                return !(bool)valeur;
            if (reglages.ContainsKey("migration")) return false;  // migré : l'ancien aria.yaml n'a plus cours
        }
        // Pas encore migré (Aria jamais lancée depuis) : la ligne qu'écrivait le lanceur dans l'ancien aria.yaml
        try
        {
            return Regex.IsMatch(File.ReadAllText(AncienFichierChoix()), @"^installation_proposee:\s*false\s*$",
                                 RegexOptions.Multiline | RegexOptions.IgnoreCase);
        }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }
    }

    // « installation.proposee = false » dans reglages.json, le reste du fichier gardé ; Aria le récrit à sa façon à la
    // prochaine écriture. Sans clé « migration », Aria y versera encore les anciens fichiers sans écraser ce choix.
    static bool NePlusProposer()
    {
        bool illisible;
        Dictionary<string, object> reglages = LireReglages(out illisible);
        if (illisible) return false;  // jamais écrasé : Aria le mettra de côté elle-même
        if (reglages == null) reglages = new Dictionary<string, object>();
        object section;
        var installation = reglages.TryGetValue("installation", out section) ? section as Dictionary<string, object> : null;
        if (installation == null) installation = new Dictionary<string, object>();
        installation["proposee"] = false;
        reglages["installation"] = installation;
        string fichier = FichierReglages(), temporaire = fichier + ".tmp";
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(fichier));
            File.WriteAllText(temporaire, new JavaScriptSerializer().Serialize(reglages), new UTF8Encoding(false));
            if (File.Exists(fichier)) File.Replace(temporaire, fichier, null);
            else File.Move(temporaire, fichier);
            return true;
        }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }
    }

    static readonly Color Fond = ColorTranslator.FromHtml("#06101f");
    static readonly Color Texte = ColorTranslator.FromHtml("#ddebff");
    static readonly Color Secondaire = ColorTranslator.FromHtml("#93a8c8");
    static readonly Color Cyan = ColorTranslator.FromHtml("#7df9ff");
    static readonly Color Orange = ColorTranslator.FromHtml("#ff8a1f");

    static DialogResult Proposer(out bool raccourci, out bool nePlus)
    {
        var f = new Form
        {
            Text = Titre, FormBorderStyle = FormBorderStyle.FixedDialog, StartPosition = FormStartPosition.CenterScreen,
            MaximizeBox = false, MinimizeBox = false, ShowInTaskbar = true, BackColor = Fond, ForeColor = Texte,
            Font = new Font("Segoe UI", 10f), ClientSize = new Size(520, 250), Padding = new Padding(20),
        };
        try { f.Icon = Icon.ExtractAssociatedIcon(Assembly.GetEntryAssembly().Location); } catch (ArgumentException) { }
        var titre = new Label
        {
            Text = "Installer Aria Studio sur cet ordinateur ?", Font = new Font("Segoe UI", 12f, FontStyle.Bold),
            AutoSize = true, Location = new Point(20, 18),
        };
        var detail = new Label
        {
            Text = "Le programme sera rangé dans " + Programme() + ", à côté de tes données. "
                   + "Rien d'autre n'est modifié. Tu peux aussi le lancer d'ici, sans rien installer.",
            ForeColor = Secondaire, Location = new Point(20, 50), Size = new Size(480, 60),
        };
        var caseRaccourci = new CheckBox { Text = "Créer un raccourci sur le Bureau", AutoSize = true, Location = new Point(20, 118) };
        var caseNePlus = new CheckBox { Text = "Ne plus me proposer", AutoSize = true, Location = new Point(20, 146) };
        var installer = new Button
        {
            Text = "Installer", DialogResult = DialogResult.Yes, FlatStyle = FlatStyle.Flat, BackColor = Orange,
            ForeColor = ColorTranslator.FromHtml("#1a0d00"), Font = new Font("Segoe UI", 10f, FontStyle.Bold),
            Size = new Size(140, 36), Location = new Point(20, 192),
        };
        installer.FlatAppearance.BorderColor = Orange;
        var sansInstaller = new Button
        {
            Text = "Lancer sans installer", DialogResult = DialogResult.No, FlatStyle = FlatStyle.Flat,
            ForeColor = Cyan, BackColor = Fond, Size = new Size(190, 36), Location = new Point(172, 192),
        };
        sansInstaller.FlatAppearance.BorderColor = Cyan;
        f.Controls.AddRange(new Control[] { titre, detail, caseRaccourci, caseNePlus, installer, sansInstaller });
        f.AcceptButton = installer;
        DialogResult r = f.ShowDialog();
        raccourci = caseRaccourci.Checked;
        nePlus = caseNePlus.Checked;
        return r;
    }

    // --- la mise à jour intégrée ---

    static int MettreAJour(string racine, string[] args)
    {
        int i = Math.Max(Array.IndexOf(args, "--mise-a-jour"), Array.IndexOf(args, "--essai-mise-a-jour"));
        int pid;
        if (i + 1 >= args.Length || !int.TryParse(args[i + 1], out pid))
        {
            Erreur("Mise à jour : il manque le numéro du serveur d'Aria à attendre.");
            return 1;
        }
        string ancien = i + 2 < args.Length ? args[i + 2] : null;
        try
        {
            using (Process serveur = Process.GetProcessById(pid))
            {
                if (!serveur.WaitForExit(120000))
                {
                    Erreur("Aria ne s'est pas arrêtée : la mise à jour n'est pas installée.");
                    return 1;
                }
            }
        }
        catch (ArgumentException) { }  // déjà arrêté
        AttendreIconeFermee(30000);  // l'icône verrouille programme\Aria-Studio.exe ; elle se ferme avec le serveur
        string exe = Installer(racine, false);
        if (essai) return exe != null ? 0 : 1;
        string relance = exe ?? (ancien != null && File.Exists(ancien) ? ancien : null);
        if (relance != null)
            Process.Start(new ProcessStartInfo(relance) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(relance) });
        return exe != null ? 0 : 1;
    }

    // --- l'installation ---

    // Renvoie l'exe installé, ou null (message déjà donné) : dans ce cas programme\ n'a pas changé.
    static string Installer(string racine, bool raccourci)
    {
        string maison = Maison(), programme = Programme();
        string nouveau = programme + ".nouveau", precedent = programme + ".precedent";
        if (MemeDossier(racine, programme) || MemeDossier(racine, precedent))
        {
            Erreur("Ce lanceur est déjà celui du dossier d'installation : rien à installer.");
            return null;
        }
        try
        {
            Directory.CreateDirectory(maison);
            foreach (string dossier in new[] { programme, precedent })
            {
                if (Directory.Exists(dossier) && !Libre(dossier))
                {
                    Erreur("Aria Studio tourne encore depuis " + dossier + ".\nQuitte-la (⏻ Quitter), puis relance.");
                    return null;
                }
            }
            if (Directory.Exists(nouveau)) Directory.Delete(nouveau, true);  // reste d'une installation interrompue
            Exception echec = Attendre("Installation d'Aria Studio en cours…", delegate { Copier(racine, nouveau); });
            if (echec != null) throw echec;

            if (Directory.Exists(programme))
            {
                if (Directory.Exists(precedent)) Directory.Delete(precedent, true);
                Directory.Move(programme, precedent);
            }
            try
            {
                Directory.Move(nouveau, programme);
            }
            catch (Exception)
            {
                if (!Directory.Exists(programme) && Directory.Exists(precedent)) Directory.Move(precedent, programme);
                throw;
            }
        }
        catch (Exception e)
        {
            try { if (Directory.Exists(nouveau)) Directory.Delete(nouveau, true); } catch (Exception) { }
            Erreur("L'installation n'a pas pu se faire : " + e.Message + "\nRien n'a changé dans " + programme + ".");
            return null;
        }

        string exe = Path.Combine(programme, NomExe());
        if (raccourci) CreerRaccourci(exe);
        return exe;
    }

    // Un dossier dont un fichier est ouvert (Aria lancée depuis là) ne se renomme pas.
    static bool Libre(string dossier)
    {
        string essaiNom = dossier + ".verif";
        try
        {
            Directory.Move(dossier, essaiNom);
            Directory.Move(essaiNom, dossier);
            return true;
        }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }
    }

    static void Copier(string source, string cible)
    {
        Directory.CreateDirectory(cible);
        foreach (string f in Directory.GetFiles(source))
        {
            string copie = Path.Combine(cible, Path.GetFileName(f));
            File.Copy(f, copie, true);
            RetirerMarque(copie);
        }
        foreach (string d in Directory.GetDirectories(source))
            Copier(d, Path.Combine(cible, Path.GetFileName(d)));
    }

    // CopyFile recopie aussi le flux « Zone.Identifier » (la marque « téléchargé depuis Internet ») : on l'efface,
    // l'utilisateur a lancé l'exe et accepté l'installation.
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, EntryPoint = "DeleteFileW", SetLastError = true)]
    static extern bool SupprimerFichier(string chemin);

    static void RetirerMarque(string fichier)
    {
        SupprimerFichier(fichier + ":Zone.Identifier");
    }

    // Une tâche longue, avec une petite fenêtre d'attente (sans fenêtre pendant les essais).
    static Exception Attendre(string message, ThreadStart tache)
    {
        Exception erreur = null;
        ThreadStart enveloppe = delegate { try { tache(); } catch (Exception e) { erreur = e; } };
        if (essai)
        {
            enveloppe();
            return erreur;
        }
        var t = new Thread(enveloppe);
        var f = new Form
        {
            Text = Titre, FormBorderStyle = FormBorderStyle.FixedDialog, StartPosition = FormStartPosition.CenterScreen,
            ControlBox = false, BackColor = Fond, ForeColor = Texte, Font = new Font("Segoe UI", 10f),
            ClientSize = new Size(420, 110),
        };
        f.Controls.Add(new Label { Text = message, AutoSize = true, Location = new Point(20, 20) });
        f.Controls.Add(new ProgressBar { Style = ProgressBarStyle.Marquee, Location = new Point(20, 56), Size = new Size(380, 18) });
        var minuterie = new System.Windows.Forms.Timer { Interval = 200 };
        minuterie.Tick += delegate { if (!t.IsAlive) { minuterie.Stop(); f.Close(); } };
        f.FormClosing += delegate(object s, FormClosingEventArgs e) { if (t.IsAlive) e.Cancel = true; };
        t.Start();
        minuterie.Start();
        f.ShowDialog();
        return erreur;
    }

    // Raccourci sur le Bureau de l'utilisateur (jamais le Bureau public), vers programme\ : la cible ne change pas
    // d'une version à l'autre. Un raccourci déjà là n'est jamais remplacé sans le demander.
    static void CreerRaccourci(string exe)
    {
        string bureau = essai
            ? Path.Combine(Environment.GetEnvironmentVariable("USERPROFILE") ?? "", "Desktop")
            : Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        string lien = Path.Combine(bureau, NomRaccourci);
        if (File.Exists(lien))
        {
            if (essai) return;
            DialogResult r = MessageBox.Show(
                "Un raccourci « Aria Studio » existe déjà sur ton Bureau.\nLe mettre à jour pour qu'il ouvre la version installée ?",
                Titre, MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (r != DialogResult.Yes) return;
        }
        object coquille = null, raccourci = null;
        try
        {
            Directory.CreateDirectory(bureau);
            Type type = Type.GetTypeFromProgID("WScript.Shell");
            coquille = Activator.CreateInstance(type);
            raccourci = type.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, coquille, new object[] { lien });
            Type t = raccourci.GetType();
            t.InvokeMember("TargetPath", BindingFlags.SetProperty, null, raccourci, new object[] { exe });
            t.InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, raccourci, new object[] { Path.GetDirectoryName(exe) });
            t.InvokeMember("Description", BindingFlags.SetProperty, null, raccourci, new object[] { "Aria Studio" });
            t.InvokeMember("IconLocation", BindingFlags.SetProperty, null, raccourci, new object[] { exe + ",0" });
            t.InvokeMember("Save", BindingFlags.InvokeMethod, null, raccourci, null);
        }
        catch (Exception e)
        {
            Erreur("Aria Studio est installée, mais le raccourci du Bureau n'a pas pu être créé : " + e.Message);
        }
        finally
        {
            if (raccourci != null && Marshal.IsComObject(raccourci)) Marshal.ReleaseComObject(raccourci);
            if (coquille != null && Marshal.IsComObject(coquille)) Marshal.ReleaseComObject(coquille);
        }
    }

    // --- le lancement ---

    // Démarre le serveur s'il ne répond pas, ouvre l'interface (sauf --silencieux), puis tient l'icône.
    static int Lancer(string racine, string url, bool silencieux)
    {
        Process serveur = null;
        if (!Repond(url))
        {
            string pythonw = Path.Combine(racine, "python", "pythonw.exe");
            if (!File.Exists(pythonw))
            {
                Erreur("Python portable introuvable :\n" + pythonw);
                return 1;
            }
            var psi = new ProcessStartInfo(pythonw, "-m app");
            psi.WorkingDirectory = racine;
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.EnvironmentVariables["PYTHONNOUSERSITE"] = "1";
            psi.EnvironmentVariables["ARIA_SANS_NAVIGATEUR"] = "1";  // c'est le lanceur qui l'ouvre, une fois prêt
            serveur = Process.Start(psi);

            DateTime limite = DateTime.Now.AddSeconds(DelaiDemarrageSecondes);
            while (!Repond(url))
            {
                if (serveur.HasExited)
                {
                    Erreur("Aria - Studio s'est arrêtée au démarrage (code " + serveur.ExitCode + ").\n"
                           + "Regarde le journal : " + Journal(racine));
                    return 1;
                }
                if (DateTime.Now > limite)
                {
                    Erreur("Aria - Studio ne répond pas.\nRegarde le journal : " + Journal(racine));
                    return 1;
                }
                Thread.Sleep(500);
            }
        }

        if (!silencieux) Process.Start(new ProcessStartInfo(Adresse(racine, url, 20)) { UseShellExecute = true });
        return TenirIcone(racine, url, serveur);
    }

    // cache\session.json du serveur qui tourne : à côté du programme en mode portable, sinon dans le dossier d'Aria
    static string FichierSession(string racine)
    {
        string dossier = Directory.Exists(Path.Combine(racine, "data")) ? racine : Maison();
        return Path.Combine(dossier, "cache", "session.json");
    }

    // L'adresse de l'interface avec le jeton de ce lancement ; sans jeton lisible (une version d'Aria d'avant le jeton),
    // l'adresse nue : une version qui l'exige montre alors sa page « Rouvre Aria avec son lanceur ».
    internal static string Adresse(string racine, string url, int essais)
    {
        for (int i = 0; i < essais; i++)
        {
            try
            {
                var session = new JavaScriptSerializer().DeserializeObject(File.ReadAllText(FichierSession(racine)))
                              as Dictionary<string, object>;
                object valeur;
                if (session != null && session.TryGetValue("jeton", out valeur) && valeur is string && ((string)valeur).Length > 0)
                    return url + "?jeton=" + Uri.EscapeDataString((string)valeur);
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
            catch (ArgumentException) { }
            catch (InvalidOperationException) { }
            if (i + 1 < essais) Thread.Sleep(250);
        }
        return url;
    }

    // Port de la section « app: » de config.yaml
    static int LirePort(string racine)
    {
        try
        {
            string config = File.ReadAllText(Path.Combine(racine, "config.yaml"));
            Match m = Regex.Match(config, @"^app:\s*\r?\n(?:[ \t]+.*\r?\n|\r?\n)*?[ \t]+port:\s*(\d+)", RegexOptions.Multiline);
            if (m.Success) return int.Parse(m.Groups[1].Value);
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
        return PortParDefaut;
    }

    // Même règle qu'app/emplacements.py : data\ à côté du programme = mode portable, sinon le dossier de l'utilisateur
    internal static string Journal(string racine)
    {
        string data = Directory.Exists(Path.Combine(racine, "data"))
            ? Path.Combine(racine, "data")
            : Path.Combine(Maison(), "data");
        return Path.Combine(data, "logs", "aria.log");
    }

    static bool Repond(string url)
    {
        try
        {
            var requete = (HttpWebRequest)WebRequest.Create(url + "api/sante");
            requete.Timeout = 1000;
            using (var reponse = (HttpWebResponse)requete.GetResponse())
            {
                return reponse.StatusCode == HttpStatusCode.OK;
            }
        }
        catch (WebException)
        {
            return false;
        }
    }

    internal static void Avertir(string message)
    {
        MessageBox.Show(message, Titre, MessageBoxButtons.OK, MessageBoxIcon.Warning);
    }

    internal static void Erreur(string message)
    {
        if (essai)
        {
            Console.Error.WriteLine(message);
            return;
        }
        MessageBox.Show(message, "Aria - Studio", MessageBoxButtons.OK, MessageBoxIcon.Error);
    }
}
