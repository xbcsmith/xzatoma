#!/usr/bin/env bash
#
#
rm -rvf ./tmp/*.txt ./tmp/*.log

rpk topic trim-prefix xzatoma.plans --offset 0 --brokers localhost:19092
rpk topic trim-prefix xzatoma.results --offset 0 --brokers localhost:19092
