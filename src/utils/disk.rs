// Filter out unnecessary disk partitions
pub fn should_include_disk(mount_point: &str) -> bool {
    // Exclude common system partitions that don't need monitoring
    let excluded_patterns = [
        "/System/Volumes/Data",                 // macOS system partition
        "/System/Volumes/VM",                   // macOS VM partition
        "/System/Volumes/Preboot",              // macOS preboot partition
        "/System/Volumes/Update",               // macOS update partition
        "/System/Volumes/xarts",                // macOS xarts partition
        "/System/Volumes/iSCPreboot",           // macOS iSC preboot partition
        "/System/Volumes/Hardware",             // macOS hardware partition
        "/System/Volumes/Data/home",            // macOS auto_home mount
        "/tmp",                                 // Temporary files
        "/var/tmp",                             // Temporary files
        "/dev",                                 // Device files
        "/proc",                                // Process files
        "/sys",                                 // System files
        "/run",                                 // Runtime files
        "/snap",                                // Snap packages
        "/boot",                                // Boot files
        "/usr",                                 // User programs
        "/var/log",                             // Log files
        "/var/spool",                           // Spool files
        "/var/lib",                             // Library files
        "/var/cache",                           // Cache files
        "/var/tmp",                             // Temporary files
        "/var/backups",                         // Backup files
        "/var/crash",                           // Crash files
        "/var/mail",                            // Mail files
        "/var/opt",                             // Optional files
        "/usr/local",                           // Local user programs
        "/usr/share",                           // Shared files
        "/usr/src",                             // Source files
        "/usr/include",                         // Include files
        "/usr/lib",                             // Library files
        "/usr/bin",                             // Binary files
        "/usr/sbin",                            // System binary files
        "/usr/libexec",                         // Executable library files
        "/Library",                             // macOS library files
        "/Applications",                        // macOS applications
        "/System",                              // macOS system files
        "/bin",                                 // Binary files
        "/sbin",                                // System binary files
        "/etc",                                 // Configuration files
        "/lib",                                 // Library files
        "/lib64",                               // 64-bit library files
        "/opt",                                 // Optional files
        "/media",                               // Media files
        "/mnt",                                 // Mount points
        "/root",                                // Root user home
        "/home",                                // User home directories
        "/srv",                                 // Service files
        "/var/run",                             // Runtime files
        "/var/lock",                            // Lock files
        "/private",                             // macOS private files
        "/Volumes",                             // macOS volumes
        "/Network",                             // macOS network files
        "/Users/Shared",                        // macOS shared user files
        "/cores",                               // Core dump files
        "/dev/shm",                             // Shared memory
        "/run/user",                            // User runtime files
        "/run/lock",                            // Lock files
        "/run/shm",                             // Shared memory
        "/run/log",                             // Log files
        "/run/systemd",                         // Systemd files
        "/run/udev",                            // Udev files
        "/run/user",                            // User runtime files
        "/run/dbus",                            // D-Bus files
        "/run/NetworkManager",                  // NetworkManager files
        "/run/cups",                            // CUPS files
        "/run/avahi-daemon",                    // Avahi daemon files
        "/run/crond",                           // Cron daemon files
        "/run/sshd",                            // SSH daemon files
        "/run/httpd",                           // HTTP daemon files
        "/run/mysqld",                          // MySQL daemon files
        "/run/postgresql",                      // PostgreSQL daemon files
        "/run/redis",                           // Redis daemon files
        "/run/memcached",                       // Memcached daemon files
        "/run/rabbitmq",                        // RabbitMQ daemon files
        "/run/elasticsearch",                   // Elasticsearch daemon files
        "/run/docker",                          // Docker daemon files
        "/run/containerd",                      // Containerd daemon files
        "/run/k8s",                             // Kubernetes daemon files
        "/run/flannel",                         // Flannel daemon files
        "/run/calico",                          // Calico daemon files
        "/run/cni",                             // CNI daemon files
        "/run/runc",                            // Runc daemon files
        "/run/crio",                            // CRI-O daemon files
        "/run/podman",                          // Podman daemon files
        "/run/buildah",                         // Buildah daemon files
        "/run/skopeo",                          // Skopeo daemon files
        "/run/kata-containers",                 // Kata containers daemon files
        "/run/firecracker",                     // Firecracker daemon files
        "/run/gvisor",                          // gVisor daemon files
        "/run/lxc",                             // LXC daemon files
        "/run/lxd",                             // LXD daemon files
        "/run/openvz",                          // OpenVZ daemon files
        "/run/vz",                              // Virtuozzo daemon files
        "/run/xen",                             // Xen daemon files
        "/run/qemu",                            // QEMU daemon files
        "/run/kvm",                             // KVM daemon files
        "/run/libvirt",                         // libvirt daemon files
        "/run/spice",                           // SPICE daemon files
        "/run/vnc",                             // VNC daemon files
        "/run/tigervnc",                        // TigerVNC daemon files
        "/run/x11vnc",                          // x11vnc daemon files
        "/run/turbovnc",                        // TurboVNC daemon files
        "/run/realvnc",                         // RealVNC daemon files
        "/run/tightvnc",                        // TightVNC daemon files
        "/run/ultravnc",                        // UltraVNC daemon files
        "/run/novnc",                           // noVNC daemon files
        "/run/websockify",                      // Websockify daemon files
        "/run/guacd",                           // Guacd daemon files
        "/run/freerdp",                         // FreeRDP daemon files
        "/run/rdesktop",                        // rdesktop daemon files
        "/run/xrdp",                            // xrdp daemon files
        "/run/x2go",                            // x2go daemon files
        "/run/nomachine",                       // NoMachine daemon files
        "/run/teamviewer",                      // TeamViewer daemon files
        "/run/anydesk",                         // AnyDesk daemon files
        "/run/chrome-remote-desktop",           // Chrome Remote Desktop daemon files
        "/run/rustdesk",                        // RustDesk daemon files
        "/run/parsec",                          // Parsec daemon files
        "/run/moonlight",                       // Moonlight daemon files
        "/run/sunshine",                        // Sunshine daemon files
        "/run/steam",                           // Steam daemon files
        "/run/lutris",                          // Lutris daemon files
        "/run/playonlinux",                     // PlayOnLinux daemon files
        "/run/bottles",                         // Bottles daemon files
        "/run/crossover",                       // CrossOver daemon files
        "/run/wine",                            // Wine daemon files
        "/run/proton",                          // Proton daemon files
        "/run/dxvk",                            // DXVK daemon files
        "/run/vkd3d",                           // VKD3D daemon files
        "/run/mesa",                            // Mesa daemon files
        "/run/vulkan",                          // Vulkan daemon files
        "/run/opencl",                          // OpenCL daemon files
        "/run/cuda",                            // CUDA daemon files
        "/run/rocm",                            // ROCm daemon files
        "/run/hip",                             // HIP daemon files
        "/run/sycl",                            // SYCL daemon files
        "/run/openvino",                        // OpenVINO daemon files
        "/run/tensorrt",                        // TensorRT daemon files
        "/run/triton",                          // Triton daemon files
        "/run/onnx",                            // ONNX daemon files
        "/run/tensorflow",                      // TensorFlow daemon files
        "/run/pytorch",                         // PyTorch daemon files
        "/run/mxnet",                           // MXNet daemon files
        "/run/caffe",                           // Caffe daemon files
        "/run/theano",                          // Theano daemon files
        "/run/keras",                           // Keras daemon files
        "/run/scikit-learn",                    // Scikit-learn daemon files
        "/run/pandas",                          // Pandas daemon files
        "/run/numpy",                           // NumPy daemon files
        "/run/scipy",                           // SciPy daemon files
        "/run/matplotlib",                      // Matplotlib daemon files
        "/run/seaborn",                         // Seaborn daemon files
        "/run/plotly",                          // Plotly daemon files
        "/run/bokeh",                           // Bokeh daemon files
        "/run/dash",                            // Dash daemon files
        "/run/streamlit",                       // Streamlit daemon files
        "/run/jupyter",                         // Jupyter daemon files
        "/run/ipython",                         // IPython daemon files
        "/run/spyder",                          // Spyder daemon files
        "/run/pycharm",                         // PyCharm daemon files
        "/run/vscode",                          // VS Code daemon files
        "/run/atom",                            // Atom daemon files
        "/run/sublime-text",                    // Sublime Text daemon files
        "/run/vim",                             // Vim daemon files
        "/run/emacs",                           // Emacs daemon files
        "/run/nano",                            // Nano daemon files
        "/run/gedit",                           // Gedit daemon files
        "/run/kate",                            // Kate daemon files
        "/run/kwrite",                          // KWrite daemon files
        "/run/notepadqq",                       // Notepadqq daemon files
        "/run/leafpad",                         // Leafpad daemon files
        "/run/mousepad",                        // Mousepad daemon files
        "/run/pluma",                           // Pluma daemon files
        "/run/xed",                             // Xed daemon files
        "/run/featherpad",                      // FeatherPad daemon files
        "/run/l3afpad",                         // L3afpad daemon files
        "/run/ted",                             // Ted daemon files
        "/run/nedit",                           // NEdit daemon files
        "/run/jed",                             // Jed daemon files
        "/run/joe",                             // Joe daemon files
        "/run/micro",                           // Micro daemon files
        "/run/kakoune",                         // Kakoune daemon files
        "/run/helix",                           // Helix daemon files
        "/run/xi",                              // Xi daemon files
        "/run/lapce",                           // Lapce daemon files
        "/run/zed",                             // Zed daemon files
        "/run/fleet",                           // Fleet daemon files
        "/run/github-desktop",                  // GitHub Desktop daemon files
        "/run/gitkraken",                       // GitKraken daemon files
        "/run/sourcetree",                      // SourceTree daemon files
        "/run/fork",                            // Fork daemon files
        "/run/gitg",                            // Gitg daemon files
        "/run/gitk",                            // Gitk daemon files
        "/run/tig",                             // Tig daemon files
        "/run/lazygit",                         // LazyGit daemon files
        "/run/gitui",                           // GitUI daemon files
        "/run/gh",                              // GitHub CLI daemon files
        "/run/glab",                            // GitLab CLI daemon files
        "/run/hub",                             // Hub daemon files
        "/run/git-lfs",                         // Git LFS daemon files
        "/run/git-annex",                       // Git Annex daemon files
        "/run/dvc",                             // DVC daemon files
        "/run/pre-commit",                      // Pre-commit daemon files
        "/run/black",                           // Black daemon files
        "/run/flake8",                          // Flake8 daemon files
        "/run/pylint",                          // Pylint daemon files
        "/run/mypy",                            // MyPy daemon files
        "/run/bandit",                          // Bandit daemon files
        "/run/safety",                          // Safety daemon files
        "/run/pipenv",                          // Pipenv daemon files
        "/run/poetry",                          // Poetry daemon files
        "/run/conda",                           // Conda daemon files
        "/run/mamba",                           // Mamba daemon files
        "/run/pip",                             // Pip daemon files
        "/run/virtualenv",                      // Virtualenv daemon files
        "/run/venv",                            // Venv daemon files
        "/run/pyenv",                           // Pyenv daemon files
        "/run/rbenv",                           // rbenv daemon files
        "/run/rvm",                             // RVM daemon files
        "/run/chruby",                          // chruby daemon files
        "/run/ruby-build",                      // ruby-build daemon files
        "/run/bundler",                         // Bundler daemon files
        "/run/gem",                             // Gem daemon files
        "/run/rails",                           // Rails daemon files
        "/run/sinatra",                         // Sinatra daemon files
        "/run/jekyll",                          // Jekyll daemon files
        "/run/middleman",                       // Middleman daemon files
        "/run/nanoc",                           // Nanoc daemon files
        "/run/hugo",                            // Hugo daemon files
        "/run/gatsby",                          // Gatsby daemon files
        "/run/next",                            // Next.js daemon files
        "/run/nuxt",                            // Nuxt.js daemon files
        "/run/sveltekit",                       // SvelteKit daemon files
        "/run/astro",                           // Astro daemon files
        "/run/remix",                           // Remix daemon files
        "/run/angular",                         // Angular daemon files
        "/run/vue",                             // Vue daemon files
        "/run/react",                           // React daemon files
        "/run/svelte",                          // Svelte daemon files
        "/run/lit",                             // Lit daemon files
        "/run/stencil",                         // Stencil daemon files
        "/run/polymer",                         // Polymer daemon files
        "/run/webcomponents",                   // Web Components daemon files
        "/run/pwa",                             // PWA daemon files
        "/run/electron",                        // Electron daemon files
        "/run/tauri",                           // Tauri daemon files
        "/run/nwjs",                            // NW.js daemon files
        "/run/cordova",                         // Cordova daemon files
        "/run/phonegap",                        // PhoneGap daemon files
        "/run/ionic",                           // Ionic daemon files
        "/run/capacitor",                       // Capacitor daemon files
        "/run/expo",                            // Expo daemon files
        "/run/react-native",                    // React Native daemon files
        "/run/flutter",                         // Flutter daemon files
        "/run/dart",                            // Dart daemon files
        "/run/kotlin",                          // Kotlin daemon files
        "/run/java",                            // Java daemon files
        "/run/scala",                           // Scala daemon files
        "/run/clojure",                         // Clojure daemon files
        "/run/groovy",                          // Groovy daemon files
        "/run/gradle",                          // Gradle daemon files
        "/run/maven",                           // Maven daemon files
        "/run/ant",                             // Ant daemon files
        "/run/sbt",                             // SBT daemon files
        "/run/leiningen",                       // Leiningen daemon files
        "/run/boot",                            // Boot daemon files
        "/run/spring",                          // Spring daemon files
        "/run/micronaut",                       // Micronaut daemon files
        "/run/quarkus",                         // Quarkus daemon files
        "/run/vertx",                           // Vert.x daemon files
        "/run/akka",                            // Akka daemon files
        "/run/play",                            // Play daemon files
        "/run/lagom",                           // Lagom daemon files
        "/run/alpakka",                         // Alpakka daemon files
        "/run/slick",                           // Slick daemon files
        "/run/doobie",                          // Doobie daemon files
        "/run/cats",                            // Cats daemon files
        "/run/scalaz",                          // Scalaz daemon files
        "/run/shapeless",                       // Shapeless daemon files
        "/run/circe",                           // Circe daemon files
        "/run/argonaut",                        // Argonaut daemon files
        "/run/spray",                           // Spray daemon files
        "/run/finagle",                         // Finagle daemon files
        "/run/twitter-util",                    // Twitter Util daemon files
        "/run/util",                            // Util daemon files
        "/run/scrooge",                         // Scrooge daemon files
        "/run/finch",                           // Finch daemon files
        "/run/finatra",                         // Finatra daemon files
        "/run/twitter-server",                  // Twitter Server daemon files
        "/run/ostrich",                         // Ostrich daemon files
        "/run/zipkin",                          // Zipkin daemon files
        "/run/jaeger",                          // Jaeger daemon files
        "/run/opentelemetry",                   // OpenTelemetry daemon files
        "/run/prometheus",                      // Prometheus daemon files
        "/run/grafana",                         // Grafana daemon files
        "/run/kibana",                          // Kibana daemon files
        "/run/logstash",                        // Logstash daemon files
        "/run/beats",                           // Beats daemon files
        "/run/fluentd",                         // Fluentd daemon files
        "/run/fluent-bit",                      // Fluent Bit daemon files
        "/run/vector",                          // Vector daemon files
        "/run/rsyslog",                         // Rsyslog daemon files
        "/run/syslog-ng",                       // syslog-ng daemon files
        "/run/journald",                        // Journald daemon files
        "/run/systemd-journal",                 // systemd-journal daemon files
        "/run/auditd",                          // Auditd daemon files
        "/run/osquery",                         // Osquery daemon files
        "/run/falco",                           // Falco daemon files
        "/run/sysdig",                          // Sysdig daemon files
        "/run/datadog",                         // Datadog daemon files
        "/run/newrelic",                        // New Relic daemon files
        "/run/dynatrace",                       // Dynatrace daemon files
        "/run/appdynamics",                     // AppDynamics daemon files
        "/run/splunk",                          // Splunk daemon files
        "/run/sumo-logic",                      // Sumo Logic daemon files
        "/run/loggly",                          // Loggly daemon files
        "/run/papertrail",                      // Papertrail daemon files
        "/run/logdna",                          // LogDNA daemon files
        "/run/logz-io",                         // Logz.io daemon files
        "/run/humio",                           // Humio daemon files
        "/run/loki",                            // Loki daemon files
        "/run/tempo",                           // Tempo daemon files
        "/run/mimir",                           // Mimir daemon files
        "/run/cortex",                          // Cortex daemon files
        "/run/thanos",                          // Thanos daemon files
        "/run/victoria-metrics",                // Victoria Metrics daemon files
        "/run/influxdb",                        // InfluxDB daemon files
        "/run/timescaledb",                     // TimescaleDB daemon files
        "/run/questdb",                         // QuestDB daemon files
        "/run/clickhouse",                      // ClickHouse daemon files
        "/run/druid",                           // Druid daemon files
        "/run/pinot",                           // Pinot daemon files
        "/run/kylin",                           // Kylin daemon files
        "/run/superset",                        // Superset daemon files
        "/run/metabase",                        // Metabase daemon files
        "/run/tableau",                         // Tableau daemon files
        "/run/looker",                          // Looker daemon files
        "/run/power-bi",                        // Power BI daemon files
        "/run/qlik",                            // Qlik daemon files
        "/run/sisense",                         // Sisense daemon files
        "/run/domo",                            // Domo daemon files
        "/run/chartio",                         // Chartio daemon files
        "/run/mode",                            // Mode daemon files
        "/run/periscope",                       // Periscope daemon files
        "/run/sigma",                           // Sigma daemon files
        "/run/hex",                             // Hex daemon files
        "/run/count",                           // Count daemon files
        "/run/observable",                      // Observable daemon files
        "/run/jupyter-book",                    // Jupyter Book daemon files
        "/run/bookdown",                        // Bookdown daemon files
        "/run/gitbook",                         // GitBook daemon files
        "/run/docusaurus",                      // Docusaurus daemon files
        "/run/mkdocs",                          // MkDocs daemon files
        "/run/sphinx",                          // Sphinx daemon files
        "/run/rustdoc",                         // Rustdoc daemon files
        "/run/javadoc",                         // Javadoc daemon files
        "/run/scaladoc",                        // Scaladoc daemon files
        "/run/godoc",                           // Godoc daemon files
        "/run/pkgdown",                         // pkgdown daemon files
        "/run/roxygen",                         // Roxygen daemon files
        "/run/doxygen",                         // Doxygen daemon files
        "/run/naturaldocs",                     // Natural Docs daemon files
        "/run/sandcastle",                      // Sandcastle daemon files
        "/run/helpndoc",                        // HelpNDoc daemon files
        "/run/madcap",                          // MadCap daemon files
        "/run/confluence",                      // Confluence daemon files
        "/run/notion",                          // Notion daemon files
        "/run/obsidian",                        // Obsidian daemon files
        "/run/roam",                            // Roam daemon files
        "/run/logseq",                          // Logseq daemon files
        "/run/dendron",                         // Dendron daemon files
        "/run/foam",                            // Foam daemon files
        "/run/athens",                          // Athens daemon files
        "/run/remnote",                         // RemNote daemon files
        "/run/anki",                            // Anki daemon files
        "/run/supermemo",                       // SuperMemo daemon files
        "/run/memrise",                         // Memrise daemon files
        "/run/quizlet",                         // Quizlet daemon files
        "/run/duolingo",                        // Duolingo daemon files
        "/run/babbel",                          // Babbel daemon files
        "/run/rosetta-stone",                   // Rosetta Stone daemon files
        "/run/pimsleur",                        // Pimsleur daemon files
        "/run/busuu",                           // Busuu daemon files
        "/run/lingoda",                         // Lingoda daemon files
        "/run/italki",                          // italki daemon files
        "/run/preply",                          // Preply daemon files
        "/run/cambly",                          // Cambly daemon files
        "/run/hellotalk",                       // HelloTalk daemon files
        "/run/tandem",                          // Tandem daemon files
        "/run/speaky",                          // Speaky daemon files
        "/run/conversation-exchange",           // Conversation Exchange daemon files
        "/run/mylanguageexchange",              // MyLanguageExchange daemon files
        "/run/bilingua",                        // Bilingua daemon files
        "/run/langcorrect",                     // LangCorrect daemon files
        "/run/writeandimprove",                 // WriteAndImprove daemon files
        "/run/grammarly",                       // Grammarly daemon files
        "/run/hemingway",                       // Hemingway daemon files
        "/run/prowritingaid",                   // ProWritingAid daemon files
        "/run/ginger",                          // Ginger daemon files
        "/run/languagetool",                    // LanguageTool daemon files
        "/run/after-the-deadline",              // After the Deadline daemon files
        "/run/whitesmoke",                      // WhiteSmoke daemon files
        "/run/slick-write",                     // SlickWrite daemon files
        "/run/paper-rater",                     // PaperRater daemon files
        "/run/online-correction",               // OnlineCorrection daemon files
        "/run/reverso",                         // Reverso daemon files
        "/run/linguee",                         // Linguee daemon files
        "/run/deepl",                           // DeepL daemon files
        "/run/google-translate",                // Google Translate daemon files
        "/run/bing-translator",                 // Bing Translator daemon files
        "/run/yandex-translate",                // Yandex Translate daemon files
        "/run/papago",                          // Papago daemon files
        "/run/baidu-translate",                 // Baidu Translate daemon files
        "/run/alibaba-translate",               // Alibaba Translate daemon files
        "/run/tencent-translate",               // Tencent Translate daemon files
        "/run/sogou-translate",                 // Sogou Translate daemon files
        "/run/iflytek-translate",               // iFlytek Translate daemon files
        "/run/niutrans",                        // NiuTrans daemon files
        "/run/systran",                         // SYSTRAN daemon files
        "/run/sdl-trados",                      // SDL Trados daemon files
        "/run/memsource",                       // Memsource daemon files
        "/run/phrase",                          // Phrase daemon files
        "/run/lokalise",                        // Lokalise daemon files
        "/run/crowdin",                         // Crowdin daemon files
        "/run/transifex",                       // Transifex daemon files
        "/run/weblate",                         // Weblate daemon files
        "/run/pontoon",                         // Pontoon daemon files
        "/run/zanata",                          // Zanata daemon files
        "/run/pootle",                          // Pootle daemon files
        "/run/virtaal",                         // Virtaal daemon files
        "/run/lokalize",                        // Lokalize daemon files
        "/run/gtranslator",                     // Gtranslator daemon files
        "/run/kbabel",                          // KBabel daemon files
        "/run/omegat",                          // OmegaT daemon files
        "/run/cafetran",                        // CafeTran daemon files
        "/run/wordfast",                        // Wordfast daemon files
        "/run/matecat",                         // MateCat daemon files
        "/run/smartcat",                        // SmartCat daemon files
        "/run/linguee-translate",               // Linguee Translate daemon files
        "/run/reverso-translate",               // Reverso Translate daemon files
        "/run/collins-translate",               // Collins Translate daemon files
        "/run/oxford-translate",                // Oxford Translate daemon files
        "/run/cambridge-translate",             // Cambridge Translate daemon files
        "/run/macmillan-translate",             // Macmillan Translate daemon files
        "/run/longman-translate",               // Longman Translate daemon files
        "/run/merriam-webster-translate",       // Merriam-Webster Translate daemon files
        "/run/dictionary-translate",            // Dictionary Translate daemon files
        "/run/urban-dictionary-translate",      // Urban Dictionary Translate daemon files
        "/run/wiktionary-translate",            // Wiktionary Translate daemon files
        "/run/wikipedia-translate",             // Wikipedia Translate daemon files
        "/run/fandom-translate",                // Fandom Translate daemon files
        "/run/reddit-translate",                // Reddit Translate daemon files
        "/run/twitter-translate",               // Twitter Translate daemon files
        "/run/facebook-translate",              // Facebook Translate daemon files
        "/run/instagram-translate",             // Instagram Translate daemon files
        "/run/tiktok-translate",                // TikTok Translate daemon files
        "/run/youtube-translate",               // YouTube Translate daemon files
        "/run/twitch-translate",                // Twitch Translate daemon files
        "/run/discord-translate",               // Discord Translate daemon files
        "/run/slack-translate",                 // Slack Translate daemon files
        "/run/teams-translate",                 // Teams Translate daemon files
        "/run/zoom-translate",                  // Zoom Translate daemon files
        "/run/skype-translate",                 // Skype Translate daemon files
        "/run/whatsapp-translate",              // WhatsApp Translate daemon files
        "/run/telegram-translate",              // Telegram Translate daemon files
        "/run/signal-translate",                // Signal Translate daemon files
        "/run/viber-translate",                 // Viber Translate daemon files
        "/run/line-translate",                  // Line Translate daemon files
        "/run/wechat-translate",                // WeChat Translate daemon files
        "/run/qq-translate",                    // QQ Translate daemon files
        "/run/dingtalk-translate",              // DingTalk Translate daemon files
        "/run/feishu-translate",                // Feishu Translate daemon files
        "/run/lark-translate",                  // Lark Translate daemon files
        "/run/tencent-meeting-translate",       // Tencent Meeting Translate daemon files
        "/run/voovmeeting-translate",           // VooV Meeting Translate daemon files
        "/run/classin-translate",               // ClassIn Translate daemon files
        "/run/tencent-docs-translate",          // Tencent Docs Translate daemon files
        "/run/jinshan-docs-translate",          // Jinshan Docs Translate daemon files
        "/run/shimo-docs-translate",            // Shimo Docs Translate daemon files
        "/run/yuque-translate",                 // Yuque Translate daemon files
        "/run/notion-translate",                // Notion Translate daemon files
        "/run/obsidian-translate",              // Obsidian Translate daemon files
        "/run/logseq-translate",                // Logseq Translate daemon files
        "/run/roam-translate",                  // Roam Translate daemon files
        "/run/dendron-translate",               // Dendron Translate daemon files
        "/run/foam-translate",                  // Foam Translate daemon files
        "/run/athens-translate",                // Athens Translate daemon files
        "/run/remnote-translate",               // RemNote Translate daemon files
        "/run/anki-translate",                  // Anki Translate daemon files
        "/run/supermemo-translate",             // SuperMemo Translate daemon files
        "/run/memrise-translate",               // Memrise Translate daemon files
        "/run/quizlet-translate",               // Quizlet Translate daemon files
        "/run/duolingo-translate",              // Duolingo Translate daemon files
        "/run/babbel-translate",                // Babbel Translate daemon files
        "/run/rosetta-stone-translate",         // Rosetta Stone Translate daemon files
        "/run/pimsleur-translate",              // Pimsleur Translate daemon files
        "/run/busuu-translate",                 // Busuu Translate daemon files
        "/run/lingoda-translate",               // Lingoda Translate daemon files
        "/run/italki-translate",                // italki Translate daemon files
        "/run/preply-translate",                // Preply Translate daemon files
        "/run/cambly-translate",                // Cambly Translate daemon files
        "/run/hellotalk-translate",             // HelloTalk Translate daemon files
        "/run/tandem-translate",                // Tandem Translate daemon files
        "/run/speaky-translate",                // Speaky Translate daemon files
        "/run/conversation-exchange-translate", // Conversation Exchange Translate daemon files
        "/run/mylanguageexchange-translate",    // MyLanguageExchange Translate daemon files
        "/run/bilingua-translate",              // Bilingua Translate daemon files
        "/run/langcorrect-translate",           // LangCorrect Translate daemon files
        "/run/writeandimprove-translate",       // WriteAndImprove Translate daemon files
        "/run/grammarly-translate",             // Grammarly Translate daemon files
        "/run/hemingway-translate",             // Hemingway Translate daemon files
        "/run/prowritingaid-translate",         // ProWritingAid Translate daemon files
        "/run/ginger-translate",                // Ginger Translate daemon files
        "/run/languagetool-translate",          // LanguageTool Translate daemon files
        "/run/after-the-deadline-translate",    // After the Deadline Translate daemon files
        "/run/whitesmoke-translate",            // WhiteSmoke Translate daemon files
        "/run/slick-write-translate",           // SlickWrite Translate daemon files
        "/run/paper-rater-translate",           // PaperRater Translate daemon files
        "/run/online-correction-translate",     // OnlineCorrection Translate daemon files
        "/run/reverso-translate",               // Reverso Translate daemon files
        "/run/linguee-translate",               // Linguee Translate daemon files
        "/run/deepl-translate",                 // DeepL Translate daemon files
        "/run/google-translate-translate",      // Google Translate Translate daemon files
        "/run/bing-translator-translate",       // Bing Translator Translate daemon files
        "/run/yandex-translate-translate",      // Yandex Translate Translate daemon files
        "/run/papago-translate",                // Papago Translate daemon files
        "/run/baidu-translate-translate",       // Baidu Translate Translate daemon files
        "/run/alibaba-translate-translate",     // Alibaba Translate Translate daemon files
        "/run/tencent-translate-translate",     // Tencent Translate Translate daemon files
        "/run/sogou-translate-translate",       // Sogou Translate Translate daemon files
        "/run/iflytek-translate-translate",     // iFlytek Translate Translate daemon files
        "/run/niutrans-translate",              // NiuTrans Translate daemon files
        "/run/systran-translate",               // SYSTRAN Translate daemon files
        "/run/sdl-trados-translate",            // SDL Trados Translate daemon files
        "/run/memsource-translate",             // Memsource Translate daemon files
        "/run/phrase-translate",                // Phrase Translate daemon files
        "/run/lokalise-translate",              // Lokalise Translate daemon files
        "/run/crowdin-translate",               // Crowdin Translate daemon files
        "/run/transifex-translate",             // Transifex Translate daemon files
        "/run/weblate-translate",               // Weblate Translate daemon files
        "/run/pontoon-translate",               // Pontoon Translate daemon files
        "/run/zanata-translate",                // Zanata Translate daemon files
        "/run/pootle-translate",                // Pootle Translate daemon files
        "/run/virtaal-translate",               // Virtaal Translate daemon files
        "/run/lokalize-translate",              // Lokalize Translate daemon files
        "/run/gtranslator-translate",           // Gtranslator Translate daemon files
        "/run/kbabel-translate",                // KBabel Translate daemon files
        "/run/omegat-translate",                // OmegaT Translate daemon files
        "/run/cafetran-translate",              // CafeTran Translate daemon files
        "/run/wordfast-translate",              // Wordfast Translate daemon files
        "/run/matecat-translate",               // MateCat Translate daemon files
        "/run/smartcat-translate",              // SmartCat Translate daemon files
    ];

    // Check if the mount point starts with any of the excluded patterns
    for pattern in &excluded_patterns {
        if mount_point.starts_with(pattern) {
            return false;
        }
    }

    // Include all other mount points
    true
}
