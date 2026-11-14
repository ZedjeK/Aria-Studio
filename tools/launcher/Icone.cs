// Icône de notification d'Aria Studio (contrat des applis Aria) : le lanceur reste ouvert après avoir ouvert
// l'interface et tient l'icône près de l'horloge, tant que le serveur répond. Une seule icône par dossier d'Aria
// (verrou nommé) ; relancer l'exe rouvre l'interface sans seconde icône.
//
// L'état se lit toutes les 2 s sur 127.0.0.1, avec le jeton de cache\session.json : /api/etat (moteurs, carte
// graphique, génération), /api/telechargements, /api/mise-a-jour. Rien ne sort de la machine.
//
// Icône délavée quand rien n'est actif, pleine quand un moteur tient la carte ; une seule pastille, dans cet ordre :
// orange (une génération attend la carte, prise par une autre appli), cyan (génération ou téléchargement en cours),
// bleue (mise à jour disponible).
//
// Démarrer avec Windows : la valeur « Aria Studio » de HKCU\Software\Microsoft\Windows\CurrentVersion\Run, relue à
// chaque ouverture du menu (app/demarrage.py écrit la même), seulement pour Aria installée dans programme\.
//
// Réservés aux essais (tests/test_lanceur.py) :
//   --essai-demarrage etat|activer|desactiver   lit ou écrit la valeur Run (clé d'essai d'ARIA_ESSAI_CLE_RUN, posée
//                                               seulement avec ARIA_TESTS_ISOLES) et écrit l'état sur la sortie
//   --essai-icone <etat.json> [<dossier>]       écrit la pastille, l'infobulle et les lignes d'état calculées depuis
//                                               un état au format des API ; avec un dossier, y dessine l'icône (PNG)
//                                               et une capture du menu ouvert

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Net;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Forms;
using Microsoft.Win32;

static partial class Launcher
{
    // --- démarrer avec Windows ---

    const string NomRun = "Aria Studio";
    const string CleRunWindows = @"Software\Microsoft\Windows\CurrentVersion\Run";

    static string CleRun()
    {
        string essaiCle = Environment.GetEnvironmentVariable("ARIA_ESSAI_CLE_RUN");
        if (!string.IsNullOrEmpty(essaiCle) && !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("ARIA_TESTS_ISOLES")))
            return essaiCle;
        return CleRunWindows;
    }

    static string ExeInstalle() { return Path.Combine(Programme(), "Aria-Studio.exe"); }
    static string CommandeRun() { return "\"" + ExeInstalle() + "\" --silencieux"; }

    // Même règle qu'app/demarrage.py : Aria installée, et pas lancée en portable.
    internal static bool DemarragePossible(string racine)
    {
        bool portable = Directory.Exists(Path.Combine(racine, "data")) && !MemeDossier(racine, Maison());
        return File.Exists(ExeInstalle()) && !portable;
    }

    internal static string LireRun()
    {
        try
        {
            using (RegistryKey k = Registry.CurrentUser.OpenSubKey(CleRun()))
            {
                return k == null ? null : k.GetValue(NomRun) as string;
            }
        }
        catch (System.Security.SecurityException) { return null; }
        catch (IOException) { return null; }
    }

    internal static bool EcrireRun(bool actif)
    {
        try
        {
            if (actif)
            {
                using (RegistryKey k = Registry.CurrentUser.CreateSubKey(CleRun()))
                {
                    k.SetValue(NomRun, CommandeRun(), RegistryValueKind.String);
                }
            }
            else
            {
                using (RegistryKey k = Registry.CurrentUser.OpenSubKey(CleRun(), true))
                {
                    if (k != null && k.GetValue(NomRun) != null) k.DeleteValue(NomRun);
                }
            }
            return true;
        }
        catch (Exception e)
        {
            Erreur("Démarrer avec Windows : le registre a refusé (" + e.Message + ").");
            return false;
        }
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    static extern bool SetProcessDPIAware();

    // Sortie des essais en UTF-8, quelle que soit la page de code (un winexe n'a pas de console)
    static void Ecrire(string texte)
    {
        byte[] octets = new UTF8Encoding(false).GetBytes(texte);
        using (Stream sortie = Console.OpenStandardOutput()) sortie.Write(octets, 0, octets.Length);
    }

    static int EssaiDemarrage(string racine, string[] args)
    {
        int i = Array.IndexOf(args, "--essai-demarrage");
        string action = i + 1 < args.Length ? args[i + 1] : "etat";
        if (action == "activer" && !DemarragePossible(racine)) return 2;
        if (action == "activer" || action == "desactiver")
        {
            if (!EcrireRun(action == "activer")) return 1;
        }
        string valeur = LireRun();
        Ecrire("possible=" + (DemarragePossible(racine) ? 1 : 0) + "\nactif=" + (valeur != null ? 1 : 0)
               + "\nvaleur=" + (valeur ?? "") + "\n");
        return 0;
    }

    // --- l'icône ---

    // Un verrou par dossier d'Aria : deux profils (un essai et le vrai) ont chacun leur icône.
    static string NomVerrou()
    {
        uint h = 2166136261;
        foreach (char c in Maison().ToLowerInvariant()) { h ^= c; h *= 16777619; }
        return @"Local\Aria-Studio-icone-" + h.ToString("x8");
    }

    // Mise à jour : programme\Aria-Studio.exe reste verrouillé tant que l'icône tourne ; elle se ferme d'elle-même
    // quand le serveur s'arrête. Vrai si elle est fermée (ou absente) dans le délai.
    static bool AttendreIconeFermee(int millisecondes)
    {
        using (var verrou = new Mutex(false, NomVerrou()))
        {
            try
            {
                if (!verrou.WaitOne(millisecondes)) return false;
            }
            catch (AbandonedMutexException) { }
            verrou.ReleaseMutex();
            return true;
        }
    }

    // Après le lancement : tient l'icône si aucune autre ne le fait, jusqu'à l'arrêt du serveur.
    static int TenirIcone(string racine, string url, Process serveur)
    {
        bool premier;
        using (var verrou = new Mutex(true, NomVerrou(), out premier))
        {
            if (!premier) return 0;  // une icône tourne déjà pour ce dossier
            try
            {
                Application.EnableVisualStyles();
                Application.Run(new Icone(racine, url, serveur));
            }
            finally
            {
                verrou.ReleaseMutex();
            }
        }
        return 0;
    }

    static int EssaiIcone(string racine, string[] args)
    {
        int i = Array.IndexOf(args, "--essai-icone");
        if (i + 1 >= args.Length) return 1;
        var etat = EtatAria.Lire(File.ReadAllText(args[i + 1], Encoding.UTF8));
        etat.DemarragePossible = DemarragePossible(racine);
        etat.DemarrageActif = LireRun() != null;
        var sortie = new StringBuilder();
        sortie.Append("pastille=" + etat.Pastille() + "\n");
        sortie.Append("pleine=" + (etat.Pleine() ? 1 : 0) + "\n");
        sortie.Append("infobulle=" + etat.Infobulle() + "\n");
        foreach (var l in etat.Lignes()) sortie.Append("ligne=" + l.Couleur + "|" + l.Texte + "\n");
        Ecrire(sortie.ToString());
        if (i + 2 < args.Length)
        {
            string dossier = args[i + 2];
            Directory.CreateDirectory(dossier);
            foreach (int taille in new[] { 16, 24, 64 })
                using (Bitmap b = Icone.Dessiner(etat.Pleine(), etat.Pastille(), taille))
                    b.Save(Path.Combine(dossier, "icone-" + taille + ".png"), ImageFormat.Png);
            SetProcessDPIAware();  // la capture lit l'écran en pixels réels
            Application.EnableVisualStyles();
            using (var icone = new Icone(racine, "http://127.0.0.1:1/", null, etat))
            {
                icone.Capturer(Path.Combine(dossier, "menu.png"));
            }
        }
        return 0;
    }
}

// Ce que l'icône montre, lu sur les API d'Aria (ou dans un fichier pour les essais).
sealed class EtatAria
{
    public struct Ligne
    {
        public string Couleur;  // "", "cyan", "orange", "bleu"
        public string Texte;
    }

    public string ComfyUI = "absent", Llama = "absent", ModeleLlm;
    public double? VramUtilisee, VramTotale;
    public string CommandeId, CommandeType, CommandeEtat, CommandeEtape;
    public double CommandeProgression;
    public int CommandeFile;
    public bool AttenteCarte, AutreAppli;
    public string Telechargement;
    public double TelechargementProgression = -1;
    public string MiseAJour;
    public bool DemarragePossible, DemarrageActif;

    static Dictionary<string, object> Objet(object o) { return o as Dictionary<string, object>; }

    static object Champ(Dictionary<string, object> d, string cle)
    {
        object v;
        return d != null && d.TryGetValue(cle, out v) ? v : null;
    }

    static double? Nombre(object o)
    {
        if (o == null) return null;
        try { return Convert.ToDouble(o, CultureInfo.InvariantCulture); }
        catch (FormatException) { return null; }
        catch (InvalidCastException) { return null; }
    }

    // Fichier d'essai : {"etat": <api/etat>, "telechargements": <api/telechargements>, "maj": <api/mise-a-jour>}
    public static EtatAria Lire(string json)
    {
        var tout = Objet(new JavaScriptSerializer().DeserializeObject(json));
        var e = new EtatAria();
        e.Remplir(Objet(Champ(tout, "etat")), Champ(tout, "telechargements") as object[], Objet(Champ(tout, "maj")));
        return e;
    }

    public void Remplir(Dictionary<string, object> etat, object[] telechargements, Dictionary<string, object> maj)
    {
        if (etat != null)
        {
            ComfyUI = Champ(Objet(Champ(etat, "comfyui")), "etat") as string ?? "absent";
            var llama = Objet(Champ(etat, "llama"));
            Llama = Champ(llama, "etat") as string ?? "absent";
            ModeleLlm = Champ(llama, "modele") as string;
            VramUtilisee = Nombre(Champ(etat, "vram_utilisee_go"));
            VramTotale = Nombre(Champ(etat, "vram_totale_go"));
            var c = Objet(Champ(etat, "commande"));
            CommandeId = Champ(c, "id") as string;
            CommandeType = Champ(c, "type") as string;
            CommandeEtat = Champ(c, "etat") as string;
            CommandeEtape = Champ(c, "etape") as string;
            CommandeProgression = Nombre(Champ(c, "progression")) ?? 0;
            CommandeFile = (int)(Nombre(Champ(c, "file")) ?? 0);
            var garde = Objet(Champ(etat, "garde"));
            AttenteCarte = Champ(garde, "attente") != null;
            AutreAppli = Champ(garde, "alerte") != null;
        }
        if (telechargements != null)
        {
            foreach (object o in telechargements)
            {
                var t = Objet(o);
                string etatT = Champ(t, "etat") as string;
                if (etatT != "en_cours" && etatT != "verification" && etatT != "attente") continue;
                Telechargement = Champ(t, "nom") as string ?? "un modèle";
                double? recu = Nombre(Champ(t, "recu")), taille = Nombre(Champ(t, "taille"));
                TelechargementProgression = recu.HasValue && taille.HasValue && taille.Value > 0 ? recu.Value / taille.Value : -1;
                if (etatT == "en_cours") break;  // celui qui avance avant ceux qui attendent
            }
        }
        if (maj != null && Champ(maj, "disponible") is bool && (bool)Champ(maj, "disponible"))
            MiseAJour = Champ(Objet(Champ(maj, "derniere")), "version") as string ?? "?";
    }

    public bool Genere { get { return CommandeId != null; } }
    public bool MoteurCharge { get { return ComfyUI == "actif" || Llama == "actif"; } }

    public bool Pleine() { return MoteurCharge || Genere || Telechargement != null; }

    public string Pastille()
    {
        if (AttenteCarte) return "orange";
        if (Genere || Telechargement != null) return "cyan";
        if (MiseAJour != null) return "bleu";
        return "";
    }

    static string Pourcent(double fraction)
    {
        return Math.Round(fraction * 100).ToString(CultureInfo.InvariantCulture) + " %";
    }

    string Travail()
    {
        string quoi;
        switch (CommandeType)
        {
            case "creation": quoi = "image"; break;
            case "sequence": quoi = "séquence"; break;
            case null: quoi = "travail"; break;
            default: quoi = CommandeType; break;
        }
        if (CommandeEtat == "attente") return quoi + " en attente";
        return (string.IsNullOrEmpty(CommandeEtape) ? quoi : CommandeEtape.ToLowerInvariant()) + " · " + Pourcent(CommandeProgression);
    }

    public List<Ligne> Lignes()
    {
        var l = new List<Ligne>();
        if (AttenteCarte) l.Add(new Ligne { Couleur = "orange", Texte = "Carte graphique : une autre appli la tient · ta génération attend" });
        if (Genere) l.Add(new Ligne { Couleur = "cyan", Texte = "Génère : " + Travail() + (CommandeFile > 1 ? " · " + CommandeFile + " en file" : "") });
        if (Telechargement != null)
            l.Add(new Ligne { Couleur = "cyan", Texte = "Téléchargement : " + Telechargement + (TelechargementProgression >= 0 ? " · " + Pourcent(TelechargementProgression) : "") });
        string vram = VramUtilisee.HasValue && VramTotale.HasValue
            ? " · " + VramUtilisee.Value.ToString("0.0", CultureInfo.GetCultureInfo("fr-FR")) + " / " + VramTotale.Value.ToString("0", CultureInfo.InvariantCulture) + " Go"
            : "";
        if (ComfyUI == "actif") l.Add(new Ligne { Couleur = "", Texte = "ComfyUI chargé" + vram });
        else if (Llama == "actif") l.Add(new Ligne { Couleur = "", Texte = "LLM chargé" + (ModeleLlm != null ? " (" + ModeleLlm + ")" : "") + vram });
        else l.Add(new Ligne { Couleur = "", Texte = "Moteurs au repos" + vram });
        if (AutreAppli && !AttenteCarte) l.Add(new Ligne { Couleur = "", Texte = "Carte graphique : une autre appli en tient une partie" });
        if (MiseAJour != null) l.Add(new Ligne { Couleur = "bleu", Texte = "Mise à jour : la " + MiseAJour + " est disponible" });
        return l;
    }

    // L'infobulle d'une icône de notification tient en 63 caractères (limite de Windows Forms).
    public string Infobulle()
    {
        string t;
        if (AttenteCarte) t = "la carte est prise, ta génération attend";
        else if (Genere) t = "génère : " + (CommandeEtat == "attente" ? "en attente" : Pourcent(CommandeProgression)) + (CommandeFile > 1 ? " · " + CommandeFile + " en file" : "");
        else if (Telechargement != null) t = "télécharge" + (TelechargementProgression >= 0 ? " : " + Pourcent(TelechargementProgression) : "");
        else if (ComfyUI == "actif") t = "ComfyUI chargé";
        else if (Llama == "actif") t = "LLM chargé";
        else if (MiseAJour != null) t = "la " + MiseAJour + " est disponible";
        else t = "rien en cours";
        string texte = "Aria Studio · " + t;
        return texte.Length <= 63 ? texte : texte.Substring(0, 62) + "…";
    }
}

sealed class Icone : ApplicationContext
{
    static readonly Color Panneau = ColorTranslator.FromHtml("#06101f");
    static readonly Color Survol = ColorTranslator.FromHtml("#0b1a30");
    static readonly Color Texte = ColorTranslator.FromHtml("#ddebff");
    static readonly Color Secondaire = ColorTranslator.FromHtml("#93a8c8");
    static readonly Color Eteint = ColorTranslator.FromHtml("#5d7092");
    static readonly Color Cyan = ColorTranslator.FromHtml("#7df9ff");
    static readonly Color Bleu = ColorTranslator.FromHtml("#5c8bff");
    static readonly Color Orange = ColorTranslator.FromHtml("#ff8a1f");
    static readonly Color Ligne = ColorTranslator.FromHtml("#1a2d4d");
    static readonly Color Barre = ColorTranslator.FromHtml("#0a1322");

    readonly string racine, url;
    readonly Process serveur;
    readonly NotifyIcon icone;
    readonly ContextMenuStrip menu;
    readonly System.Windows.Forms.Timer minuterie;
    readonly SynchronizationContext contexte;
    readonly Dictionary<string, Icon> icones = new Dictionary<string, Icon>();
    EtatAria etat = new EtatAria();
    int echecs, enCours;
    bool quitte, fini;

    public Icone(string racine, string url, Process serveur) : this(racine, url, serveur, null) { }

    public Icone(string racine, string url, Process serveur, EtatAria essai)
    {
        this.racine = racine;
        this.url = url;
        this.serveur = serveur;
        menu = new ContextMenuStrip { Renderer = new Rendu(), ShowImageMargin = true, ShowCheckMargin = false };
        menu.Font = new Font("Segoe UI", 9.5f);
        menu.Opening += delegate { Construire(); };
        contexte = SynchronizationContext.Current ?? new WindowsFormsSynchronizationContext();
        icone = new NotifyIcon { ContextMenuStrip = menu, Text = "Aria Studio", Visible = essai == null };
        icone.MouseClick += delegate(object s, MouseEventArgs e) { if (e.Button == MouseButtons.Left) Ouvrir(null); };
        if (essai != null)
        {
            etat = essai;
            Peindre();
            return;
        }
        Peindre();
        minuterie = new System.Windows.Forms.Timer { Interval = 2000 };
        minuterie.Tick += delegate { Relever(); };
        minuterie.Start();
        Relever();
    }

    // --- dessin de l'icône : le logo de Studio (aria-studio.ico, joint au lanceur), délavé ou plein, et la pastille ---

    // Le logo à la taille demandée, tiré de l'icône jointe à la compilation (/resource) ; null sans elle. Ses images
    // sont des PNG (format d'icône de Vista), que Icon.ToBitmap() de .NET Framework lit mal : on lit le répertoire du
    // .ico et on décode l'image PNG la plus proche (au-dessus de la taille, sinon la plus grande).
    static Bitmap Logo(int taille)
    {
        byte[] ico;
        using (Stream flux = System.Reflection.Assembly.GetExecutingAssembly().GetManifestResourceStream("aria-studio.ico"))
        {
            if (flux == null) return null;
            ico = new byte[flux.Length];
            int lu = 0;
            while (lu < ico.Length) lu += flux.Read(ico, lu, ico.Length - lu);
        }
        int nombre = BitConverter.ToUInt16(ico, 4), choisie = -1, cote = 0;
        for (int i = 0; i < nombre; i++)
        {
            int c = ico[6 + 16 * i] == 0 ? 256 : ico[6 + 16 * i];
            bool mieux = choisie < 0 || (c >= taille && (cote < taille || c < cote)) || (cote < taille && c > cote);
            if (mieux) { choisie = i; cote = c; }
        }
        if (choisie < 0) return null;
        int longueur = BitConverter.ToInt32(ico, 6 + 16 * choisie + 8), debut = BitConverter.ToInt32(ico, 6 + 16 * choisie + 12);
        try
        {
            using (var image = Image.FromStream(new MemoryStream(ico, debut, longueur)))
            {
                var b = new Bitmap(taille, taille, PixelFormat.Format32bppArgb);
                using (Graphics g = Graphics.FromImage(b))
                {
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.DrawImage(image, new Rectangle(0, 0, taille, taille));
                }
                return b;
            }
        }
        catch (ArgumentException) { return null; }  // pas un PNG : l'esquisse prend le relais
    }

    public static Bitmap Dessiner(bool pleine, string pastille, int taille)
    {
        var b = new Bitmap(taille, taille, PixelFormat.Format32bppArgb);
        using (Graphics g = Graphics.FromImage(b))
        using (Bitmap logo = Logo(taille))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            if (logo != null)
            {
                var attributs = new ImageAttributes();
                attributs.SetColorMatrix(new ColorMatrix { Matrix33 = pleine ? 1f : 0.4f });  // délavée : rien d'actif
                g.DrawImage(logo, new Rectangle(0, 0, taille, taille), 0, 0, logo.Width, logo.Height, GraphicsUnit.Pixel, attributs);
            }
            g.ScaleTransform(taille / 48f, taille / 48f);
            int a = pleine ? 255 : 100;
            if (logo == null) Esquisse(g, a);
            Color? couleur = pastille == "orange" ? Orange : pastille == "cyan" ? Cyan : pastille == "bleu" ? (Color?)Bleu : null;
            if (couleur.HasValue)
            {
                using (var bord = new SolidBrush(Barre)) g.FillEllipse(bord, 29, 29, 19, 19);
                using (var point = new SolidBrush(couleur.Value)) g.FillEllipse(point, 32, 32, 13, 13);
            }
        }
        return b;
    }

    // Sans l'icône jointe (compilation sans /resource) : le « A » et son clap, dessinés.
    static void Esquisse(Graphics g, int a)
    {
        var cadre = new[] { new PointF(14, 2), new PointF(34, 2), new PointF(46, 14), new PointF(46, 34), new PointF(34, 46), new PointF(14, 46), new PointF(2, 34), new PointF(2, 14) };
        using (var fond = new SolidBrush(Color.FromArgb(a, Panneau))) g.FillPolygon(fond, cadre);
        using (var trait = new Pen(Color.FromArgb(a, Bleu), 3f)) g.DrawPolygon(trait, cadre);
        using (var lettre = new Pen(Color.FromArgb(a, Cyan), 5f) { LineJoin = LineJoin.Round, StartCap = LineCap.Round, EndCap = LineCap.Round })
            g.DrawLines(lettre, new[] { new PointF(13, 37), new PointF(24, 10), new PointF(35, 37) });
        using (var clap = new SolidBrush(Color.FromArgb(a, Orange)))
        {
            g.FillRectangle(clap, 18, 29, 12, 8);  // le clap, sous la pointe du « A »
            var bascule = g.Save();
            g.TranslateTransform(18, 27.5f);
            g.RotateTransform(-18);
            g.FillRectangle(clap, 0, -2.5f, 12, 2.5f);
            g.Restore(bascule);
        }
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    static extern bool DestroyIcon(IntPtr handle);

    Icon IconePour(bool pleine, string pastille)
    {
        string cle = (pleine ? "p" : "d") + pastille;
        Icon i;
        if (!icones.TryGetValue(cle, out i))
        {
            using (Bitmap b = Dessiner(pleine, pastille, SystemInformation.SmallIconSize.Width))
            {
                IntPtr h = b.GetHicon();
                using (Icon brute = Icon.FromHandle(h)) i = (Icon)brute.Clone();  // une copie qui possède son handle
                DestroyIcon(h);
            }
            icones[cle] = i;
        }
        return i;
    }

    void Peindre()
    {
        icone.Icon = IconePour(etat.Pleine(), etat.Pastille());
        icone.Text = etat.Infobulle();
    }

    // --- lecture de l'état, hors du fil de l'interface ---

    void Relever()
    {
        if (fini || Interlocked.CompareExchange(ref enCours, 1, 0) != 0) return;
        ThreadPool.QueueUserWorkItem(delegate
        {
            EtatAria lu = null;
            try
            {
                var serialiseur = new JavaScriptSerializer { MaxJsonLength = int.MaxValue };
                var e = new EtatAria();
                var etatApi = serialiseur.DeserializeObject(Lire("api/etat")) as Dictionary<string, object>;
                object[] telechargements = null;
                Dictionary<string, object> maj = null;
                try { telechargements = serialiseur.DeserializeObject(Lire("api/telechargements")) as object[]; } catch (WebException) { }
                try { maj = serialiseur.DeserializeObject(Lire("api/mise-a-jour")) as Dictionary<string, object>; } catch (WebException) { }
                e.Remplir(etatApi, telechargements, maj);
                lu = e;
            }
            catch (WebException) { }
            catch (ArgumentException) { }
            catch (InvalidOperationException) { }
            contexte.Post(delegate { Recu(lu); enCours = 0; }, null);
        });
    }

    void Recu(EtatAria lu)
    {
        if (fini) return;
        if (lu != null)
        {
            echecs = 0;
            etat = lu;
            Peindre();
            return;
        }
        echecs++;
        bool mort = serveur != null && serveur.HasExited;
        if (mort && !quitte && serveur.ExitCode != 0)
        {
            Finir("Aria Studio s'est arrêtée (code " + serveur.ExitCode + "). Regarde le journal : " + Launcher.Journal(racine));
            return;
        }
        if (mort || echecs >= 3) Finir(null);  // quittée (menu ou ⏻ de la page) : l'icône s'en va
    }

    void Finir(string message)
    {
        fini = true;
        if (minuterie != null) minuterie.Stop();
        if (message == null)
        {
            ExitThread();
            return;
        }
        icone.ShowBalloonTip(8000, "Aria Studio", message, ToolTipIcon.Error);
        var attente = new System.Windows.Forms.Timer { Interval = 8000 };
        attente.Tick += delegate { attente.Stop(); ExitThread(); };
        attente.Start();
    }

    string Jeton()
    {
        string adresse = Launcher.Adresse(racine, url, 1);
        int i = adresse.IndexOf("?jeton=", StringComparison.Ordinal);
        return i < 0 ? null : Uri.UnescapeDataString(adresse.Substring(i + 7));
    }

    string Lire(string chemin) { return Appeler("GET", chemin); }

    string Appeler(string methode, string chemin)
    {
        var requete = (HttpWebRequest)WebRequest.Create(url + chemin);
        requete.Method = methode;
        requete.Timeout = 3000;
        requete.Proxy = null;  // 127.0.0.1 : jamais par un proxy
        string jeton = Jeton();
        if (jeton != null) requete.Headers["x-aria-jeton"] = jeton;
        if (methode == "POST")
        {
            requete.ContentType = "application/json";
            requete.ContentLength = 2;
            using (Stream s = requete.GetRequestStream()) s.Write(new[] { (byte)'{', (byte)'}' }, 0, 2);
        }
        using (var reponse = (HttpWebResponse)requete.GetResponse())
        using (var lecteur = new StreamReader(reponse.GetResponseStream(), Encoding.UTF8))
        {
            return lecteur.ReadToEnd();
        }
    }

    // --- le menu ---

    ToolStripMenuItem Entree(string texte, EventHandler clic)
    {
        var e = new ToolStripMenuItem(texte);
        if (clic != null) e.Click += clic;
        return e;
    }

    ToolStripMenuItem Titre(string texte)
    {
        var t = new ToolStripMenuItem(texte.ToUpperInvariant()) { Enabled = false, Tag = "titre" };
        t.Font = new Font("Consolas", 7.5f);
        return t;
    }

    ToolStripMenuItem LigneEtat(EtatAria.Ligne l)
    {
        Color c = l.Couleur == "cyan" ? Cyan : l.Couleur == "orange" ? Orange : l.Couleur == "bleu" ? Bleu : ColorTranslator.FromHtml("#3a4d70");
        var point = new Bitmap(16, 16);
        using (Graphics g = Graphics.FromImage(point))
        using (var pinceau = new SolidBrush(c))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.FillEllipse(pinceau, 4, 4, 8, 8);
        }
        var e = new ToolStripMenuItem(l.Texte) { Enabled = false, Image = point, Tag = "etat:" + l.Couleur };
        e.Font = new Font("Consolas", 8.5f);
        return e;
    }

    void Construire()
    {
        etat.DemarragePossible = Launcher.DemarragePossible(racine);
        etat.DemarrageActif = Launcher.LireRun() != null;  // relu dans Windows à chaque ouverture
        menu.Items.Clear();
        var ouvrir = Entree("Ouvrir Aria Studio", delegate { Ouvrir(null); });
        ouvrir.Font = new Font(menu.Font, FontStyle.Bold);
        menu.Items.Add(ouvrir);
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Titre("État"));
        foreach (var l in etat.Lignes()) menu.Items.Add(LigneEtat(l));
        if (etat.MiseAJour != null) menu.Items.Add(Entree("Voir la mise à jour", delegate { Ouvrir("mise-a-jour"); }));

        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Titre("Agir sans ouvrir"));
        var annuler = Entree("Annuler la génération en cours", delegate { Annuler(); });
        annuler.Enabled = etat.Genere;
        menu.Items.Add(annuler);
        var liberer = Entree("Libérer la carte graphique", delegate { Liberer(); });
        liberer.Enabled = !etat.Genere && (etat.ComfyUI == "actif" || etat.Llama == "actif");
        if (etat.Genere) liberer.ShortcutKeyDisplayString = "après la génération";
        menu.Items.Add(liberer);

        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Titre("Aller à"));
        foreach (var page in new[] { new[] { "Plateau", "plateau" }, new[] { "Montage", "montage" }, new[] { "Modèles", "modeles" }, new[] { "Réglages", "reglages" } })
        {
            string vue = page[1];
            menu.Items.Add(Entree(page[0], delegate { Ouvrir(vue); }));
        }

        menu.Items.Add(new ToolStripSeparator());
        var demarrer = Entree("Démarrer avec Windows", delegate { BasculerDemarrage(); });
        demarrer.Image = Case(etat.DemarrageActif, etat.DemarragePossible || etat.DemarrageActif);
        demarrer.Enabled = etat.DemarragePossible || etat.DemarrageActif;  // on peut toujours retirer la valeur
        if (!etat.DemarragePossible && !etat.DemarrageActif) demarrer.ShortcutKeyDisplayString = "une fois installée";
        menu.Items.Add(demarrer);
        menu.Items.Add(Entree("Quitter Aria Studio", delegate { Quitter(); }));
    }

    // La case de « Démarrer avec Windows » : un carré, plein cyan avec sa coche quand c'est activé (comme l'interface)
    static Bitmap Case(bool cochee, bool active)
    {
        var b = new Bitmap(16, 16);
        using (Graphics g = Graphics.FromImage(b))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            var r = new Rectangle(1, 1, 13, 13);
            if (cochee)
            {
                using (var fond = new SolidBrush(Cyan)) g.FillRectangle(fond, r);
                using (var coche = new Pen(Panneau, 2f))
                    g.DrawLines(coche, new[] { new Point(4, 8), new Point(7, 11), new Point(12, 4) });
            }
            else
            {
                using (var bord = new Pen(active ? Secondaire : ColorTranslator.FromHtml("#3a4d70"))) g.DrawRectangle(bord, r);
            }
        }
        return b;
    }

    // Capture du menu ouvert, pour les essais : on le montre au milieu de l'écran et on photographie son cadre.
    public void Capturer(string fichier)
    {
        Construire();
        menu.Show(new Point(200, 120));
        Application.DoEvents();
        Thread.Sleep(300);
        Application.DoEvents();
        Rectangle r = menu.Bounds;
        using (var b = new Bitmap(r.Width, r.Height))
        {
            using (Graphics g = Graphics.FromImage(b)) g.CopyFromScreen(r.Location, Point.Empty, r.Size);
            b.Save(fichier, ImageFormat.Png);
        }
        menu.Close();
    }

    // --- les actions ---

    void Ouvrir(string vue)
    {
        string adresse = Launcher.Adresse(racine, url, 1);
        if (vue != null) adresse += (adresse.Contains("?") ? "&" : "?") + "vue=" + vue;
        try { Process.Start(new ProcessStartInfo(adresse) { UseShellExecute = true }); }
        catch (Exception e) { Launcher.Avertir("Le navigateur n'a pas pu s'ouvrir : " + e.Message); }
    }

    void Agir(string chemin, string echec)
    {
        ThreadPool.QueueUserWorkItem(delegate
        {
            string erreur = null;
            try { Appeler("POST", chemin); }
            catch (WebException e) { erreur = Detail(e); }
            contexte.Post(delegate
            {
                if (erreur != null) Launcher.Avertir(echec + " : " + erreur);
                Relever();
            }, null);
        });
    }

    static string Detail(WebException e)
    {
        var r = e.Response as HttpWebResponse;
        if (r == null) return e.Message;
        try
        {
            using (var lecteur = new StreamReader(r.GetResponseStream(), Encoding.UTF8))
            {
                var d = new JavaScriptSerializer().DeserializeObject(lecteur.ReadToEnd()) as Dictionary<string, object>;
                object detail;
                if (d != null && d.TryGetValue("detail", out detail) && detail is string) return (string)detail;
            }
        }
        catch (Exception) { }
        return e.Message;
    }

    void Annuler()
    {
        if (etat.CommandeId == null) return;
        Agir("api/commandes/" + Uri.EscapeDataString(etat.CommandeId) + "/annuler", "L'annulation n'a pas pu se faire");
    }

    void Liberer()
    {
        Agir("api/moteurs/liberer", "La carte graphique n'a pas pu être libérée");
    }

    void BasculerDemarrage()
    {
        Launcher.EcrireRun(!(Launcher.LireRun() != null));
    }

    void Quitter()
    {
        if (etat.Genere)
        {
            DialogResult r = MessageBox.Show("Une génération est en cours. Quitter l'annule.\nQuitter Aria Studio quand même ?",
                                             "Aria Studio", MessageBoxButtons.YesNo, MessageBoxIcon.Warning);
            if (r != DialogResult.Yes) return;
        }
        quitte = true;
        Agir("api/quitter", "Aria n'a pas pu s'arrêter");
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            icone.Visible = false;
            icone.Dispose();
            menu.Dispose();
            if (minuterie != null) minuterie.Dispose();
            foreach (Icon i in icones.Values) i.Dispose();
        }
        base.Dispose(disposing);
    }

    // --- un menu Windows aux couleurs d'Aria ---

    sealed class Couleurs : ProfessionalColorTable
    {
        public override Color ToolStripDropDownBackground { get { return Panneau; } }
        public override Color ImageMarginGradientBegin { get { return Panneau; } }
        public override Color ImageMarginGradientMiddle { get { return Panneau; } }
        public override Color ImageMarginGradientEnd { get { return Panneau; } }
        public override Color MenuBorder { get { return Cyan; } }
        public override Color MenuItemBorder { get { return Survol; } }
        public override Color MenuItemSelected { get { return Survol; } }
        public override Color SeparatorDark { get { return Ligne; } }
        public override Color SeparatorLight { get { return Ligne; } }
    }

    sealed class Rendu : ToolStripProfessionalRenderer
    {
        public Rendu() : base(new Couleurs()) { RoundedEdges = false; }

        protected override void OnRenderItemText(ToolStripItemTextRenderEventArgs e)
        {
            string tag = e.Item.Tag as string;
            if (tag == "titre") e.TextColor = Secondaire;
            else if (tag == "etat:orange") e.TextColor = Orange;
            else if (tag != null && tag.StartsWith("etat:")) e.TextColor = tag == "etat:" ? Secondaire : Texte;
            else e.TextColor = e.Item.Enabled ? Texte : Eteint;
            // Dessiné ici : le rendu de base grise d'office le texte d'une entrée inactive (titres, lignes d'état)
            TextRenderer.DrawText(e.Graphics, e.Text, e.TextFont, e.TextRectangle, e.TextColor, e.TextFormat);
        }

        // Les points de couleur des lignes d'état gardent leur couleur (le rendu de base grise l'image d'une entrée inactive)
        protected override void OnRenderItemImage(ToolStripItemImageRenderEventArgs e)
        {
            if (e.Image != null) e.Graphics.DrawImage(e.Image, e.ImageRectangle);
        }

        protected override void OnRenderMenuItemBackground(ToolStripItemRenderEventArgs e)
        {
            if (!e.Item.Enabled) return;  // titres et lignes d'état : pas de survol
            base.OnRenderMenuItemBackground(e);
        }
    }
}
