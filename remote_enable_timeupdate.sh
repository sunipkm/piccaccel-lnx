#!/bin/bash
echo "# set time without root" | sudo tee /etc/sudoers.d/set-time-without-root > /dev/null
echo "Cmnd_Alias SET_TIME=/bin/date" | sudo tee -a /etc/sudoers.d/set-time-without-root > /dev/null
echo "ALL ALL=(ALL) NOPASSWD: SET_TIME" | sudo tee -a /etc/sudoers.d/set-time-without-root > /dev/null
echo "set-time-without-root installed successfully."