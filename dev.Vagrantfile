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

  config.vm.define "devn1" do |node|
    node.vm.box = "nanocl-dev"
    node.vm.provider :libvirt do |domain|
      domain.memory = 4096
      domain.cpus = 4
    end
  end

  # config.vm.define "devn2" do |node|
  #   node.vm.box = "nanocl-dev"
  #   node.vm.provider :libvirt do |domain|
  #     domain.memory = 4096
  #     domain.cpus = 4
  #   end
  # end

  # config.vm.define "devn3" do |node|
  #   node.vm.box = "nanocl-dev"
  #   node.vm.provider :libvirt do |domain|
  #     domain.memory = 4096
  #     domain.cpus = 4
  #   end
  # end
end
