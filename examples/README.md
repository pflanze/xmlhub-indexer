To set this up via crontab and using the alternative local home directory on bs-stadler09:

    $ crontab -l
    PATH=/local0/cjaege/.cargo/bin:/usr/share/Modules/bin:/usr/local/bin:/usr/bin:/usr/local/sbin:/usr/sbin:/opt/puppetlabs/bin:/home/cjaege/bin
    HOME=/local0/cjaege

    * * * * * HOME=/local0/cjaege /local0/cjaege/start-xmlhub-via-crontab

(The second `HOME` setting is probably not necessary.)
