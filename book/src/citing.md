# Citing and license

## Citing Rekkefly

If you use Rekkefly in published research, please cite it. The repository's
[`CITATION.cff`](https://github.com/pvnkmrksk/rekkefly/blob/main/CITATION.cff)
is read by GitHub (**Cite this repository**), Zotero and other tools:

```text
Kaushik, P. K. Rekkefly: the new Kinefly. Real-time wing tracking for tethered
insects (version 0.6.1). https://github.com/pvnkmrksk/rekkefly
```

```bibtex
@software{rekkefly,
  author  = {Kaushik, Pavan Kumar},
  title   = {Rekkefly: the new Kinefly. Real-time wing tracking for tethered insects},
  version = {0.6.1},
  url     = {https://github.com/pvnkmrksk/rekkefly},
  license = {MIT}
}
```

Please also cite the version you used (`rekkefly --version`), and describe the
detector (`wing_blur` or `edge`) and the marker placement in your methods. As
Rekkefly builds on Kinefly's method, consider citing Kinefly too.

## License

Rekkefly is free software under the
[MIT License](https://github.com/pvnkmrksk/rekkefly/blob/main/LICENSE): use,
copy, modify and distribute it for any purpose, including commercial use,
keeping the copyright notice.

The packages include the licenses of every dependency (`THIRD_PARTY_NOTICES.txt`).
The website uses the [Inter](https://rsms.me/inter/) typeface under the SIL
Open Font License.

## Acknowledgements

Rekkefly exists because of [Kinefly](https://github.com/ssafarik/Kinefly), by
Steve Safarik and colleagues, which defined the method and conventions it
keeps, and [Kinefly Docker](https://github.com/pvnkmrksk/Kinefly_Docker), which
kept Kinefly running and defined the ZMQ message.
