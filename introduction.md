# 🚀 Wake - Kubernetes Log Tailing Reinvented

> **Wake: Because your logs should work for you, not against you.** 🚀

## 🎉 Excited to Share Something I've Been Building!

I created **Wake** — a powerful Rust-based CLI tool that transforms how you manage and analyze logs across multiple Kubernetes pods and containers. After countless hours of frustration with traditional log management, I built Wake to solve the pain points every Kubernetes developer faces.

🌐 **Live Demo**: [wakelog.in](https://www.wakelog.in)  
⭐ **GitHub**: [github.com/samba-rgb/wake](https://github.com/samba-rgb/wake)

---

## 😤 The Kubernetes Logging Problem

If you work with Kubernetes, you know these pain points all too well:

- **🪟 Multiple terminal windows** for tracking different pod logs
- **🔄 Constant restarts** when you need to change filters
- **🤯 Complex kubectl commands** you can never remember
- **🐌 Slow diagnostics** when debugging containers
- **📊 No centralized view** across environments
- **😵‍💫 Log chaos** during incident response

**There had to be a better way...**

---

## ✨ Meet Wake: The Solution You've Been Waiting For

Wake transforms Kubernetes logging with a modern, intelligent approach:

### 🎮 **Interactive UI Mode**
```bash
wake --ui  # Real-time filtering without restarts
```
- **Dynamic filtering** - Change patterns on the fly
- **Live pattern updates** - See results instantly
- **Smart scrolling** - Never lose your place
- **Visual feedback** - Clear indication of what's happening

### 🧠 **Advanced Log Filtering**  
```bash
wake -i 'error && "payment"'           # Errors in payment service
wake -i '(info || warn) && !debug'     # Info/warnings, no debug  
wake -i '"failed" || "timeout"'        # Failed or timeout events
```
- **Logical operators** - AND, OR, NOT support
- **Real-time processing** - Filter as logs stream
- **Pattern history** - Navigate previous filters
- **Smart boundaries** - Old logs preserved

### 🌐 **Web Mode - Browser-Based Log Viewing**
```bash
wake --web  # View logs from your browser
```
- **Team collaboration** - Share log views with colleagues
- **OpenObserve integration** - Professional web interface
- **Remote access** - Monitor from anywhere
- **Persistent sessions** - Logs saved for later analysis

### 🎯 **Diagnostic Templates**
```bash
wake --exec-template jfr --template-args 1234 30s    # Java profiling
wake --exec-template heap-dump --template-args 1234  # Memory analysis
wake --exec-template thread-dump --template-args 1234 # Thread analysis
```
- **One-click diagnostics** - JFR, heap dumps, thread dumps
- **Multi-pod execution** - Run across entire cluster
- **Live monitoring** - Real-time progress tracking
- **Auto-download** - Files saved locally

### 📜 **Script Execution**
```bash
wake --script-in ./health-check.sh  # Run custom scripts in pods
```
- **Custom diagnostics** - Run your own maintenance scripts
- **Bulk operations** - Execute across multiple pods
- **Output collection** - Results saved locally
- **Error handling** - Robust execution with detailed logs

### 🔍 **Smart Command History**
```bash
wake --his "error logs"  # TF-IDF powered search
```
- **Intelligent search** - Find commands by meaning, not just text
- **Pattern suggestions** - Get relevant command examples
- **History persistence** - Commands saved across sessions
- **Context awareness** - Understands what you're looking for

---

## ⚡ Quick Start

### Install (macOS)
```bash
brew install samba-rgb/wake/wake
```

### Basic Usage
```bash
# Monitor all pods in current namespace
wake

# Filter errors in production
wake -n production -i "error"

# Interactive UI with live filtering
wake --ui

# Monitor specific app with advanced filtering
wake "my-app" -i '(error || warn) && !"debug"' --ui
```

---

## 🎯 Perfect For

| **Role** | **Use Case** |
|----------|--------------|
| **👨‍💻 DevOps Engineers** | Monitor deployments, debug distributed systems |
| **🔧 SREs** | Incident response, performance monitoring |  
| **👩‍💻 Developers** | Debug applications, analyze behavior patterns |
| **⚙️ Platform Engineers** | Manage large Kubernetes clusters efficiently |

---

## 🚀 Why Wake Stands Out

| **Feature** | **kubectl logs** | **stern** | **🏆 Wake** |
|-------------|------------------|-----------|-------------|
| Interactive UI | ❌ | ❌ | ✅ |
| Real-time filtering | ❌ | Limited | ✅ |
| Logical operators | ❌ | ❌ | ✅ |
| Web interface | ❌ | ❌ | ✅ |
| Diagnostic templates | ❌ | ❌ | ✅ |
| Script execution | ❌ | ❌ | ✅ |
| Command history | ❌ | ❌ | ✅ |
| Performance | Slow | Good | **Blazing** ⚡ |

---

## 🛠️ Built with Rust

Wake is crafted in **Rust** for:
- **🔥 Performance** - Handle massive log volumes
- **🛡️ Reliability** - Memory safety and zero crashes  
- **⚡ Speed** - Multi-threaded processing
- **📦 Easy deployment** - Single binary, no dependencies

---

## 📱 Spread the Word

**Love Wake?** Help others discover it:

### 🐦 **Twitter/X Post**
```
🚀 Just discovered Wake - it's completely changed how I debug Kubernetes! 

✨ Real-time log filtering + interactive UI = debugging bliss
🌐 Web mode for team collaboration  
🎯 Built-in diagnostics (JFR, heap dumps)
📊 Advanced filtering with logical operators

Built in Rust for speed & reliability 🦀

#Kubernetes #DevOps #Wake #Rust
```

### 💼 **LinkedIn Post**
```
🚀 Exciting tool discovery: Wake is revolutionizing Kubernetes log monitoring!

Key features that caught my attention:
• Interactive UI with real-time filtering 
• Web-based log viewing and collaboration
• Advanced filtering (AND/OR/NOT operators)
• Built-in diagnostic templates
• Script execution across pods
• Intelligent command history

The interactive UI alone makes debugging distributed systems so much more efficient. Game changer for any DevOps team working with Kubernetes!

Built in Rust for performance and reliability. 

Check it out: https://www.wakelog.in
#Kubernetes #SRE #DevOps #LogManagement
```

### 💬 **Slack/Discord Message**
```
Check out Wake - it's like stern but with superpowers! 🚀

• Interactive UI (change filters without restarting!)
• Web mode for browser-based log viewing  
• Advanced filtering with logical operators
• Built-in diagnostics (JFR, heap dumps, etc.)
• Script execution inside pods
• Smart command history with search

Makes K8s debugging actually enjoyable: https://github.com/samba-rgb/wake

Built in Rust, so it's blazing fast ⚡
```

---

## 🤝 Join the Community

### **🌟 Show Your Support**
If you find Wake useful, please consider:
- **⭐ Starring the repo** - [github.com/samba-rgb/wake](https://github.com/samba-rgb/wake)
- **🔄 Sharing with colleagues** - Help spread the word
- **🐛 Reporting issues** - Help make Wake better
- **💡 Suggesting features** - Share your ideas

### **📞 Get in Touch**
- **🌐 Website**: [wakelog.in](https://www.wakelog.in)
- **📧 Email**: [samba24052001@gmail.com](mailto:samba24052001@gmail.com)
- **🐛 Issues**: [GitHub Issues](https://github.com/samba-rgb/wake/issues)
- **📚 Documentation**: Run `wake --guide` for interactive help

---

## 🔗 Quick Links

- 🌐 **Website**: [wakelog.in](https://www.wakelog.in)
- ⭐ **GitHub**: [github.com/samba-rgb/wake](https://github.com/samba-rgb/wake)
- 🍺 **Install**: `brew install samba-rgb/wake/wake`
- 📖 **Guide**: `wake --guide`
- 🐛 **Report Issues**: [GitHub Issues](https://github.com/samba-rgb/wake/issues)

---

**🚀 Ready to revolutionize your Kubernetes logging?** 

**[Get started now!](https://www.wakelog.in)**

---

*Wake: Because your logs should work for you, not against you.* ✨

**Would love your feedback!** If you try Wake, please let me know what you think. Your input helps make it better for everyone. 🙌