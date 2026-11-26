<script lang="ts">
  import { BookOpen, ChevronDown, ChevronUp, Cpu, Network, Globe, Terminal } from "lucide-svelte";

  export let defaultOpen = false;
  let expanded = defaultOpen;
</script>

<div class="rounded-xl bg-slate-900/70 border border-slate-800/90 overflow-hidden">
  <button
    type="button"
    on:click={() => (expanded = !expanded)}
    class="w-full px-3 py-2.5 flex items-center justify-between text-left hover:bg-slate-800/50 transition cursor-pointer"
  >
    <span class="flex items-center gap-2 text-[11px] font-semibold text-cyan-300">
      <BookOpen class="w-3.5 h-3.5 shrink-0" />
      <span>Wake-on-LAN Setup Guide (BIOS, OS, MAC &amp; Free DDNS)</span>
    </span>
    {#if expanded}
      <ChevronUp class="w-3.5 h-3.5 text-slate-400 shrink-0" />
    {:else}
      <ChevronDown class="w-3.5 h-3.5 text-slate-400 shrink-0" />
    {/if}
  </button>

  {#if expanded}
    <div class="px-3 pb-3 pt-1.5 border-t border-slate-800/80 space-y-3 text-[11px] text-slate-300 leading-relaxed">
      <!-- Step 1: BIOS/UEFI -->
      <div class="space-y-1">
        <p class="font-semibold text-white flex items-center gap-1.5">
          <Cpu class="w-3.5 h-3.5 text-amber-400 shrink-0" />
          <span>1. Enable Wake-on-LAN in BIOS / UEFI</span>
        </p>
        <ul class="list-disc pl-4 space-y-0.5 text-slate-400">
          <li>Reboot and enter BIOS/UEFI (<code class="text-slate-200">Del</code> / <code class="text-slate-200">F2</code>).</li>
          <li>Enable <strong>Wake on LAN</strong>, <strong>Power On By PCI-E</strong>, or <strong>PME Event Wake Up</strong> (usually under <em>Advanced &rarr; APM / Power Management</em>).</li>
          <li>Disable <strong>ErP Ready / Deep Sleep</strong> so the network port stays powered while shut down.</li>
        </ul>
      </div>

      <!-- Step 2: OS Adapter -->
      <div class="space-y-1">
        <p class="font-semibold text-white flex items-center gap-1.5">
          <Network class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
          <span>2. Configure Your OS Network Adapter (Ethernet Recommended)</span>
        </p>
        <ul class="list-disc pl-4 space-y-0.5 text-slate-400">
          <li>
            <strong>Windows:</strong> Open <em>Device Manager &rarr; Network Adapters &rarr; Properties</em>:
            <ul class="list-circle pl-4 mt-0.5 space-y-0.5">
              <li><em>Power Management</em> tab: check <strong>Allow this device to wake the computer</strong> &amp; <strong>Only allow a magic packet</strong>.</li>
              <li><em>Advanced</em> tab: set <strong>Wake on Magic Packet</strong> = <em>Enabled</em>.</li>
              <li>Recommended: Disable <strong>Fast Startup</strong> in <em>Control Panel &rarr; Power Options &rarr; Choose what the power buttons do</em>.</li>
            </ul>
          </li>
          <li>
            <strong>Linux:</strong> Enable WoL on your interface with <code class="text-slate-200">sudo ethtool -s eth0 wol g</code> (replace <code class="text-slate-200">eth0</code> with your NIC name).
          </li>
          <li>
            <strong>macOS:</strong> Open <em>System Settings &rarr; Battery / Energy</em> and enable <strong>Wake for network access</strong>.
          </li>
        </ul>
      </div>

      <!-- Step 3: Finding MAC Address -->
      <div class="space-y-1">
        <p class="font-semibold text-white flex items-center gap-1.5">
          <Terminal class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
          <span>3. How to Find a MAC Address</span>
        </p>
        <p class="text-slate-400">
          RayDrop automatically detects this PC's MAC address above and auto-fills MAC addresses for your signed-in devices. To check manually:
        </p>
        <ul class="list-disc pl-4 space-y-0.5 text-slate-400">
          <li><strong>Windows:</strong> Run <code class="text-slate-200">getmac /v</code> or <code class="text-slate-200">ipconfig /all</code> (look for <em>Physical Address</em>).</li>
          <li><strong>Linux:</strong> Run <code class="text-slate-200">ip link show</code> (look for <code class="text-slate-200">link/ether</code>).</li>
          <li><strong>macOS:</strong> Run <code class="text-slate-200">ifconfig en0 | grep ether</code>.</li>
        </ul>
      </div>

      <!-- Step 4: WAN & Free DDNS -->
      <div class="space-y-1">
        <p class="font-semibold text-white flex items-center gap-1.5">
          <Globe class="w-3.5 h-3.5 text-purple-400 shrink-0" />
          <span>4. Wake Over Internet (WAN) &amp; Free DDNS</span>
        </p>
        <p class="text-slate-400">
          Leave <strong>WAN Host / DDNS</strong> empty for local Wi-Fi/LAN. To wake your PC from outside your home network:
        </p>
        <ol class="list-decimal pl-4 space-y-0.5 text-slate-400">
          <li>Assign a <strong>Static Local IP</strong> (DHCP Reservation) to your PC in your router settings.</li>
          <li>Set <strong>Port Forwarding</strong> on your router: external <strong>UDP Port 9</strong> &rarr; <code class="text-slate-200">Target_PC_IP:9</code> (also bind <strong>Static ARP</strong> if your router supports it).</li>
          <li>
            If your ISP uses a dynamic public IP, set up a <strong>Free DDNS</strong> domain:
            <ul class="list-disc pl-4 mt-0.5 space-y-0.5">
              <li><strong>DuckDNS</strong> (<code>duckdns.org</code>) &mdash; 100% free, no monthly confirmation, easy setup.</li>
              <li><strong>No-IP</strong> (<code>noip.com</code>) &mdash; Free up to 3 hostnames, built into most routers.</li>
              <li><strong>Dynu</strong> (<code>dynu.com</code>) &mdash; Free dynamic DNS with lightweight updater client.</li>
            </ul>
          </li>
          <li>Enter your DDNS hostname (e.g. <code class="text-slate-200">mypc.duckdns.org</code>) and UDP port above.</li>
        </ol>
      </div>
    </div>
  {/if}
</div>
