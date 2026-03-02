// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "forge-std/console.sol";

contract ConsoleTest {
    function doSomething() public {
        console.log("hello");
        console2.log("world");
    }
}
