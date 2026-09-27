import { invoke, $ } from "./shared";
import { applyThemeFromSettings } from "./shared";

async function boot() {
  await applyThemeFromSettings();
  const params = new URLSearchParams(location.search);
  const url = params.get("u") || "";
  const reason = params.get("reason") || "";
  $("#url").textContent = url;
  if (reason.startsWith("lookalike:")) {
    const rest = reason.slice("lookalike:".length);
    const brand = rest.split(":")[0] || "";
    const detail = rest.split(":").slice(1).join(":");
    $("#title").textContent = `This site may be impersonating ${brand}`;
    $("#detail").textContent = detail || "This domain looks like a well-known brand but is not it. Attackers use lookalike domains for phishing.";
  } else if (reason === "malware") {
    $("#title").textContent = "Kestrel blocked a dangerous site";
    $("#detail").textContent = "This host appears on a malware blocklist (URLhaus). Loading it could infect your device.";
  }
  $("#back").addEventListener("click", () => {
    history.back();
  });
  $("#ignore").addEventListener("click", () => {
    if (confirm(`Ignore the warning and open ${url} for this session?`)) {
      invoke("allow_dangerous_site", { url });
    }
  });
}

boot();
