// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract   SimpleContract   {
    uint256   public   value;
    address public   owner;

    constructor(   uint256 _value  )  {
        value =    _value;
        owner   = msg.sender;
    }

    function   setValue(   uint256 _value   )    external   {
        value  =  _value;
    }

    function   getValue(  )   external   view   returns   (  uint256  )   {
        return   value;
    }
}
