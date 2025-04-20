If you have a VPN that you launch to access some website, but you don't want to use it as default gateway, this tool will analyze website traffic and generate precise Wireguard rules.

## Steps:

1. Open your website in a Chrome-based browser
2. Open _Developer Tools_ and reload the page, observe how new traffic comes in to the _Network_ tab
3. Select _Export HAR (Sanitized)_ in a toolbar and save the traffic into a file
4. Nagivate to other pages on the website and store the traffic similar way. On highly-dynamic pages you'll probably notice that new traffic will dump into the same collection, so you can rewrite the previously exported file with the file containing more data. Otherwise, store several .har files, it won't make any harm
5. In the end you'll have a bunch of files like _example.com.har_, _account.example.com.har_, _news.example.com.har_, and so on
6. Download the tool (assuming [Rust](https://rustup.rs) toolchain is installed) and run it as follows:
    
    `cargo run --release /path/to/files/*.har`
7. Wait a couple of seconds (or minutes, if the dumps are big) and see the Wireguard statement prepared like this:
    
    `AllowedIPs = 219.1.0.0/16, 193.10.0.0/16`
8. As a bonus, the program will also detect if any of the ongoing TCP or UDP connections would fall into some of the generated ranges, and skip those ranges. This is to prevent the VPN tunnel to absorb unrelated traffic.


## Limitations (TODO)

- Currently the tool converts every endpoint found in dumps into a IPv4 subnet of /16, which is kinda stupid. Ideally we should query the _Whois_ service (RIPE or ARIN) and obtain the precise ASNs
- Parallel processing of dumps with [rayon](https://github.com/rayon-rs/rayon)
- Support directives of other VPN types, not only Wireguard
- More unit-tests
- "Validation" mode, like checking that every endpoint does really go into the VPN established
- Exterminate some residual `unwrap()`s, better error handling