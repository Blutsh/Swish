> [!IMPORTANT]
> **UPLOAD FEATURE IS BACK** woohoo
>
> Infomaniak retired the API this tool used to speak.
> Download path has been rewritten against the new endpoints. Both forms are accepted.
>
> Upload has also been ported to the new API. It now needs a sender address
> pass one with `-e`, and the first time you use it
> you will be asked for the 6-character code they email you. The confirmation is
> cached, so later uploads do not ask again.


> [!NOTE]
> Most of the code has been handwritten until v2. Now that agentic coding LLM AI stuff has taken over the whole world, (and I dont want to put some of my super valuable time in this project anymore) most of the update and bugfix will be adressed by a beep-bop bot that probably write 1000x times faster than me. Feelsbad, but works.

# Swissfer

A Command Line Tool for [Infomaniak](https://www.swisstransfer.com/)'s SwissTransfer Service. 🇨🇭

This project is not affiliated with Infomaniak and is not an official tool !


## Installation

All the binaries are available in the [releases](https://github.com/Blutsh/swissfer/releases) section. 

Download the binary for your platform and add it to your PATH.

### Arch User Repository

Install with yay (or any other AUR helper tool):

```
yay -S swissfer
```

## Usage

```sh
Usage: swissfer [OPTIONS] <FILE>

Arguments:
  <FILE>  could be a file or a folder or a link

Options:
  -p, --password <password>    Sets the password for the file(s) downloaded / uploaded
  -e, --email <you@example.com>  Sender address for uploads, confirmed once by email
  -m, --message <Hello World>  Define the message for the file(s) uploaded
  -n, --number-download <250>  Max downloads: 1, 20, 100, 200 or 250
  -d, --duration <30>          Days available: 1, 3, 7, 15 or 30
  -o, --output <output>        Define an output directory for the downloaded files
  -v, --verbose                Enable verbose mode
  -h, --help                   Print help
  -V, --version                Print version
```

### Examples

Upload a file
```sh
swissfer -e you@example.com /tmp/super-file.pdf
```
Upload a file with a password
```sh
swissfer -e you@example.com -p mypassword /tmp/super-file.pdf
```
Download a file
```sh
swissfer https://www.swisstransfer.com/dl/188be047-5b8c-48bf-9c4a-e70076c0e53c
```
Download a file with a password
```sh
swissfer -p mypassword https://www.swisstransfer.com/dl/188be047-5b8c-48bf-9c4a-e70076c0e53c
```
> [!IMPORTANT]  
> This tool does not provide any encryption feature. 
> I strongly recommend you to encrypt your file before uploading it to any cloud service 

## Known Issues

***Sender address verification***

SwissTransfer will not open a transfer for an address it has not confirmed. The
first upload from a given address prompts for a code sent by email. Requesting
codes too often is rate limited per IP.

***File Upload Limit***

When uploading many times the ***same*** file, it seems that infomaniaks servers flags you as suspicous traffic and you won't be able to upload for a while. It seems to be linked to your IP address, no extensive testing has been done. Use the debug mode to see the error message.

## Contributing

If a developer who truly has the skills and doesn't face the same skill issues as me wants to contribute to this project, feel free to do so. PRs and stuff. :handshake:

## License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details. 
## Contact

You can contact me at compote.interroge.0i@icloud.com :email:

## Donate
I drank some during the project, it would be nice if you could &nbsp;&nbsp;<a href='https://ko-fi.com/Y8Y7ZK4DS' target='_blank'><img height='36' style='border:0px;height:36px;' src='https://storage.ko-fi.com/cdn/kofi3.png?v=3' border='0' alt='Buy Me a Coffee at ko-fi.com' /></a>



