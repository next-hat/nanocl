# -*- mode: ruby -*-
# vi: set ft=ruby :

# All Vagrant configuration is done below. The "2" in Vagrant.configure
# configures the configuration version (we support older styles for
# backwards compatibility). Please don't change it unless you know what
# you're doing.
Vagrant.configure("2") do |config|
  # The most common configuration options are documented and commented below.
  # For a complete reference, please see the online documentation at
  # https://docs.vagrantup.com.
  # Every Vagrant development environment requires a box. You can search for
  # boxes at https://vagrantcloud.com/search.

  config.vm.synced_folder ".", "/vagrant"

  # Setup docker and build tools
  config.vm.provision "shell", inline: <<-SHELL
    set -e -u -o pipefail
    apt update -y
    apt-get install -y ca-certificates curl gnupg git gcc wget pkg-config
    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    chmod a+r /etc/apt/keyrings/docker.gpg
    echo \
      "deb [arch="$(dpkg --print-architecture)" signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
      "$(. /etc/os-release && echo "$VERSION_CODENAME")" stable" | \
    tee /etc/apt/sources.list.d/docker.list > /dev/null
    apt-get update -y
    apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
    systemctl enable docker
  SHELL

  # Setup rust
  config.vm.provision "shell", privileged: false, inline: <<-SHELL
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    echo "export PATH=$PATH:$HOME/.cargo/bin" >> ~/.bashrc
  SHELL

  config.vm.define "node1" do |node1|
    node1.vm.box = "debian/bullseye64"
    node1.vm.provider :libvirt do |domain|
      domain.memory = 4096
      domain.cpus = 4
    end
  end

  config.vm.define "node2" do |node2|
    node2.vm.box = "debian/bullseye64"
    node2.vm.provider :libvirt do |domain|
      domain.memory = 4096
      domain.cpus = 4
    end
  end

  config.vm.define "node3" do |node3|
    node3.vm.box = "debian/bullseye64"
    node3.vm.provider :libvirt do |domain|
      domain.memory = 4096
      domain.cpus = 4
    end
  end
end
