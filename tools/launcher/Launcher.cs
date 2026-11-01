// Lanceur d'Aria - Studio : démarre le serveur sans console (Python portable du dossier, « pythonw -m app »),
// attend qu'il réponde, puis ouvre l'interface dans le navigateur par défaut. Si Aria tourne déjà, il ouvre juste
// l'interface. Compilé par tools/build_portable.py avec le csc.exe fourni par Windows.

using System;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Text.RegularExpressions;
using System.Threading;
using System.Windows.Forms;

static class Launcher
{
    const int PortParDefaut = 8075;
    const int DelaiDemarrageSecondes = 90;

    [STAThread]
    static int Main()
    {
        string racine = AppDomain.CurrentDomain.BaseDirectory;
        string url = "http://127.0.0.1:" + LirePort(racine) + "/";

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
            Process serveur = Process.Start(psi);

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

        Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
        return 0;
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
    static string Journal(string racine)
    {
        string data = Directory.Exists(Path.Combine(racine, "data"))
            ? Path.Combine(racine, "data")
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".Aria-Studio", "data");
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

    static void Erreur(string message)
    {
        MessageBox.Show(message, "Aria - Studio", MessageBoxButtons.OK, MessageBoxIcon.Error);
    }
}
